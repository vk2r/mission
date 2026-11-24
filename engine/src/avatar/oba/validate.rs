//! Definition validation and body analysis (ported from `validate.ts`).

use serde_json::Value;

use super::color::js_round;
use super::path::{
    Bounds, PathError, Point, bounds, filled_area, flatten_path, inside_row, is_inside, js_num, parse_path,
    serialize_path, split_subpaths,
};
use super::types::{AvatarDefinition, AvatarEyes};

/// Neutral eye size at scale 1, viewBox units.
pub const EYE_W: f64 = 4.0;
pub const EYE_H: f64 = 11.0;
/// Hard limits; the contract's soft target for the body is 8..92.
pub const BODY_MIN: f64 = 6.0;
pub const BODY_MAX: f64 = 94.0;
const EYE_MARGIN: f64 = 1.2;
/// Max gaze travel of the eyes inside the face at scale 1. Deliberately small:
/// a subtle shift reads as looking around, and the enlarged eyes stay well
/// inside the face instead of sliding toward its edge.
pub const GAZE_X: f64 = 1.5;
pub const GAZE_Y: f64 = 1.1;

/// Happy-eye arc, as fractions of the drawn eye size (w = width, h = height):
/// a quadratic from (x ∓ halfWidth·w, y + base·h) through the control point
/// (x, y − peak·h), stroked `stroke`·w wide with round caps.
pub struct HappyArc {
    pub half_width: f64,
    pub base: f64,
    pub peak: f64,
    pub stroke: f64,
}
pub const HAPPY_ARC: HappyArc = HappyArc { half_width: 0.72, base: 0.1, peak: 0.42, stroke: 0.6 };
/// Gap the renderer keeps between every drawn eye shape and the outline while the eyes look around.
pub const EYE_CLEARANCE: f64 = 1.5;

const TOP_KEYS: [&str; 5] = ["version", "id", "name", "body", "eyes"];
const EYE_KEYS: [&str; 4] = ["x", "y", "spacing", "scale"];

#[derive(Debug, Clone)]
pub struct BodyHole {
    /// The hole's own subpath, serialized.
    pub d: String,
    /// Approximate cut-out area, units².
    pub area: f64,
}

#[derive(Debug, Clone)]
pub struct BodySubpath {
    /// The subpath, serialized.
    pub d: String,
    /// Its own flattened polygon(s).
    pub polys: Vec<Vec<Point>>,
    /// True when the subpath cuts a hole rather than adding fill.
    pub hole: bool,
    /// A point inside the subpath's own fill (for holes: inside the cut-out).
    pub probe: Option<Point>,
}

#[derive(Debug, Clone)]
pub struct BodyAnalysis {
    pub polys: Vec<Vec<Point>>,
    pub bounds: Bounds,
    pub area: usize,
    pub holes: Vec<BodyHole>,
    pub subpaths: Vec<BodySubpath>,
}

/// Holes up to this area count as small details (buttons, sockets). The renderer cuts every hole through.
pub const RECESS_MAX_AREA: f64 = 110.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GazeRange {
    pub left: f64,
    pub right: f64,
    pub up: f64,
    pub down: f64,
}

/// JS `Math.sign`: 0 stays 0.
fn sign(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Points on the outline of an eye pill expanded by `margin`.
fn eye_outline(cx: f64, cy: f64, scale: f64, margin: f64) -> Vec<Point> {
    let hw = (EYE_W * scale) / 2.0 + margin;
    let hh = (EYE_H * scale) / 2.0 + margin;
    let mut pts = Vec::new();
    let straight = hh - hw; // half-length of the straight flank
    for k in 0..16 {
        let a = (f64::from(k) / 16.0) * std::f64::consts::PI * 2.0;
        let x = a.cos() * hw;
        let y = a.sin() * hw + sign(a.sin()) * straight.max(0.0);
        pts.push([cx + x, cy + y]);
    }
    for f in [-0.5, 0.0, 0.5] {
        pts.push([cx - hw, cy + f * straight]);
        pts.push([cx + hw, cy + f * straight]);
    }
    pts.push([cx, cy]);
    pts
}

/// Outline points of every shape the renderer may draw for one eye (pill and happy arc), expanded by `margin`.
fn drawn_eye_outline(cx: f64, cy: f64, scale: f64, margin: f64) -> Vec<Point> {
    let mut pts = eye_outline(cx, cy, scale, margin);
    let w = EYE_W * scale;
    let h = EYE_H * scale;
    let x0 = cx - HAPPY_ARC.half_width * w;
    let x1 = cx + HAPPY_ARC.half_width * w;
    let yb = cy + HAPPY_ARC.base * h;
    let yc = cy - HAPPY_ARC.peak * h;
    let r = (HAPPY_ARC.stroke * w) / 2.0 + margin;
    for i in 0..=8 {
        let t = f64::from(i) / 8.0;
        let m = 1.0 - t;
        let x = m * m * x0 + 2.0 * m * t * cx + t * t * x1;
        let y = (m * m + t * t) * yb + 2.0 * m * t * yc;
        for k in 0..8 {
            let a = (f64::from(k) / 8.0) * std::f64::consts::PI * 2.0;
            pts.push([x + a.cos() * r, y + a.sin() * r]);
        }
    }
    pts
}

pub fn eye_centers(eyes: &AvatarEyes) -> [Point; 2] {
    [[eyes.x - eyes.spacing / 2.0, eyes.y], [eyes.x + eyes.spacing / 2.0, eyes.y]]
}

pub fn eyes_fit(polys: &[Vec<Point>], eyes: &AvatarEyes, dx: f64, dy: f64, margin: f64) -> Option<Point> {
    for [cx, cy] in eye_centers(eyes) {
        for [x, y] in eye_outline(cx + dx, cy + dy, eyes.scale, margin) {
            if !is_inside(polys, x, y) {
                return Some([x, y]);
            }
        }
    }
    None
}

/// Like `eyes_fit`, but for everything the renderer draws: the pill and the wider happy arc.
pub fn drawn_eyes_fit(polys: &[Vec<Point>], eyes: &AvatarEyes, dx: f64, dy: f64, margin: f64) -> Option<Point> {
    for [cx, cy] in eye_centers(eyes) {
        for [x, y] in drawn_eye_outline(cx + dx, cy + dy, eyes.scale, margin) {
            if !is_inside(polys, x, y) {
                return Some([x, y]);
            }
        }
    }
    None
}

/// Largest gap (viewBox units, to 0.1) between the drawn eyes at rest and the outline; 0 when they touch it.
pub fn eye_clearance(polys: &[Vec<Point>], eyes: &AvatarEyes) -> f64 {
    if drawn_eyes_fit(polys, eyes, 0.0, 0.0, 0.0).is_some() {
        return 0.0;
    }
    let mut lo = 0.0;
    let mut hi = 20.0;
    while hi - lo > 0.05 {
        let mid = (lo + hi) / 2.0;
        if drawn_eyes_fit(polys, eyes, 0.0, 0.0, mid).is_some() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    (lo * 10.0).floor() / 10.0
}

/// How far (0..1 of GAZE_X/GAZE_Y) the drawn eyes can travel each way and keep
/// EYE_CLEARANCE from the outline.
pub fn gaze_range(polys: &[Vec<Point>], eyes: &AvatarEyes) -> GazeRange {
    let reach = |ux: f64, uy: f64| {
        for k in [1.0, 0.8, 0.6, 0.4, 0.2] {
            if drawn_eyes_fit(polys, eyes, ux * k * GAZE_X * eyes.scale, uy * k * GAZE_Y * eyes.scale, EYE_CLEARANCE)
                .is_none()
            {
                return k;
            }
        }
        0.0
    };
    GazeRange { left: reach(-1.0, 0.0), right: reach(1.0, 0.0), up: reach(0.0, -1.0), down: reach(0.0, 1.0) }
}

pub fn analyze_body(path: &str) -> Result<BodyAnalysis, PathError> {
    let segs = parse_path(path)?;
    let polys = flatten_path(&segs);
    let b = bounds(&polys);
    let mut holes = Vec::new();
    let mut subpaths = Vec::new();
    for sub in split_subpaths(&segs) {
        let own = flatten_path(&sub);
        if own.is_empty() {
            continue;
        }
        let ob = bounds(&own);
        let mut inside = 0.0f64;
        let mut cut = 0.0f64;
        let mut probe: Option<Point> = None;
        let mut cut_probe: Option<Point> = None;
        // Small subpaths (buttons, sockets) need a fine grid; big ones classify fine on a coarser one.
        let step = if (ob.max_x - ob.min_x) * (ob.max_y - ob.min_y) < 900.0 { 0.5 } else { 1.0 };
        let alone = own.len() == polys.len();
        let mut xs = Vec::new();
        let mut x = ob.min_x.floor() + step / 2.0;
        while x < ob.max_x {
            xs.push(x);
            x += step;
        }
        let mut mine = vec![0u8; xs.len()];
        let mut all = vec![0u8; xs.len()];
        let mut y = ob.min_y.floor() + step / 2.0;
        while y < ob.max_y {
            inside_row(&own, y, &xs, &mut mine);
            if !alone {
                inside_row(&polys, y, &xs, &mut all);
            }
            for k in 0..xs.len() {
                if mine[k] == 0 {
                    continue;
                }
                inside += 1.0;
                probe.get_or_insert([xs[k], y]);
                if !alone && all[k] == 0 {
                    cut += 1.0;
                    cut_probe.get_or_insert([xs[k], y]);
                }
            }
            y += step;
        }
        cut *= step * step * 4.0;
        inside *= step * step * 4.0;
        let d = serialize_path(&sub);
        // A subpath that is mostly empty under the full fill rule is a cut-out.
        let hole = inside > 0.0 && cut / inside > 0.6;
        if hole {
            holes.push(BodyHole { d: d.clone(), area: js_round(cut / 4.0) });
        }
        subpaths.push(BodySubpath { d, polys: own, hole, probe: if hole { cut_probe } else { probe } });
    }
    let area = filled_area(&polys, b);
    Ok(BodyAnalysis { polys, bounds: b, area, holes, subpaths })
}

fn r1(n: f64) -> f64 {
    js_round(n * 10.0) / 10.0
}

fn finite(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|n| n.is_finite())
}

fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    let ok = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty() && b.len() <= 40 && ok(b[0]) && ok(b[b.len() - 1]) && b.iter().all(|c| ok(*c) || *c == b'-')
}

/// Validate a JSON definition; errors are one message per problem.
pub fn validate_avatar(input: &Value) -> Result<AvatarDefinition, Vec<String>> {
    let mut errors: Vec<String> = Vec::new();
    let Some(obj) = input.as_object() else {
        return Err(vec!["definition must be a JSON object".to_string()]);
    };

    for k in obj.keys() {
        if !TOP_KEYS.contains(&k.as_str()) {
            errors.push(format!("unknown field \"{k}\" (allowed: {})", TOP_KEYS.join(", ")));
        }
    }
    if obj.get("version").and_then(Value::as_f64) != Some(1.0) {
        errors.push("version must be 1".to_string());
    }

    let id = obj.get("id").and_then(Value::as_str);
    if !id.is_some_and(valid_id) {
        errors.push("id must be a lowercase slug of 1-40 chars [a-z0-9-]".to_string());
    }
    let name = obj.get("name").and_then(Value::as_str).map(str::trim).unwrap_or("");
    if name.is_empty()
        || name.encode_utf16().count() > 40
        || name.chars().any(|c| c <= '\u{1f}' || c == '<' || c == '>')
    {
        errors.push("name must be 1-40 printable characters without < or >".to_string());
    }

    let mut path: Option<&str> = None;
    match obj.get("body").and_then(Value::as_object) {
        None => errors.push("body must be an object { \"path\": \"...\" }".to_string()),
        Some(body) => {
            for k in body.keys() {
                if k != "path" {
                    errors.push(format!("unknown field \"body.{k}\" (only \"path\" is allowed)"));
                }
            }
            match body.get("path").and_then(Value::as_str) {
                None => errors.push("body.path must be a string".to_string()),
                Some(p) => path = Some(p),
            }
        }
    }

    let mut eyes: Option<AvatarEyes> = None;
    match obj.get("eyes").and_then(Value::as_object) {
        None => errors.push("eyes must be an object { x, y, spacing, scale }".to_string()),
        Some(e) => {
            for k in e.keys() {
                if !EYE_KEYS.contains(&k.as_str()) {
                    errors.push(format!("unknown field \"eyes.{k}\" (allowed: {})", EYE_KEYS.join(", ")));
                }
            }
            let bad: Vec<&str> = EYE_KEYS.iter().copied().filter(|k| finite(e.get(*k)).is_none()).collect();
            if !bad.is_empty() {
                errors.push(format!("eyes.{} must be finite numbers", bad.join(", eyes.")));
            } else {
                let g = |k: &str| finite(e.get(k)).unwrap_or(0.0);
                let ey = AvatarEyes { x: g("x"), y: g("y"), spacing: g("spacing"), scale: g("scale") };
                if ey.scale < 0.6 || ey.scale > 1.8 {
                    errors.push(format!("eyes.scale {} out of range 0.6..1.8", js_num(ey.scale)));
                }
                let min_spacing = EYE_W * ey.scale * 1.75;
                if ey.spacing < min_spacing || ey.spacing > 40.0 {
                    errors.push(format!(
                        "eyes.spacing {} out of range {}..40 for scale {}",
                        js_num(ey.spacing),
                        js_num(r1(min_spacing)),
                        js_num(ey.scale)
                    ));
                }
                eyes = Some(ey);
            }
        }
    }

    let mut normalized = String::new();
    let mut body: Option<BodyAnalysis> = None;
    if let Some(p) = path {
        match parse_path(p).and_then(|segs| {
            normalized = serialize_path(&segs);
            analyze_body(&normalized)
        }) {
            Ok(b) => body = Some(b),
            Err(err) => errors.push(format!("body.path: {err}")),
        }
    }

    if let Some(body) = &body {
        let b = body.bounds;
        let mut out: Vec<String> = Vec::new();
        if b.min_x < BODY_MIN {
            out.push(format!("left edge x={}", js_num(r1(b.min_x))));
        }
        if b.min_y < BODY_MIN {
            out.push(format!("top edge y={}", js_num(r1(b.min_y))));
        }
        if b.max_x > BODY_MAX {
            out.push(format!("right edge x={}", js_num(r1(b.max_x))));
        }
        if b.max_y > BODY_MAX {
            out.push(format!("bottom edge y={}", js_num(r1(b.max_y))));
        }
        if !out.is_empty() {
            errors.push(format!("body.path leaves the safe area ({}); keep geometry within 8..92", out.join(", ")));
        }
        let w = b.max_x - b.min_x;
        let h = b.max_y - b.min_y;
        if w.max(h) < 40.0 || w.min(h) < 14.0 {
            errors.push(format!(
                "body is too small ({}x{}); use at least 40 units on the long side and 14 on the short side",
                js_num(r1(w)),
                js_num(r1(h))
            ));
        }
        if body.area < 600 {
            errors.push(format!("body fill area {} is too thin to carry eyes; must be at least 600 units²", body.area));
        }
        if let Some(ey) = &eyes
            && let Some(miss) = eyes_fit(&body.polys, ey, 0.0, 0.0, EYE_MARGIN)
        {
            errors.push(format!(
                "eyes must sit inside the body with {} units margin; point ({}, {}) is outside or in a hole. \
                 Move eyes.x/y toward the widest solid area, reduce spacing or scale.",
                js_num(EYE_MARGIN),
                js_num(r1(miss[0])),
                js_num(r1(miss[1]))
            ));
        }
    }

    match (errors.is_empty(), eyes, id) {
        (true, Some(eyes), Some(id)) => {
            Ok(AvatarDefinition { version: 1, id: id.to_string(), name: name.to_string(), body_path: normalized, eyes })
        }
        _ => Err(errors),
    }
}
