//! Renderers for a validated silhouette + eye placement with implied volume (see volume.rs). One
//! flat body color, two plain dark eyes; no gradients on the body, gloss, rim light or shadows.
//! Output is built only from numbers and the renderer's own markup (ported from `render.ts`).
//!
//! - `avatar_frame`: one frame flattened to plain data (paths, matrices, eyes), for any vector
//!   renderer.
//! - `render_avatar_svg`: a standalone still, serialized from `avatar_frame`. Cross-sections of
//!   the stack that faces the viewer are projected orthographically and painted back to front.
//!
//! The SVG sticks to what QtSvg paints: `<defs>`/`<use>`, `<g transform=matrix>`, filled and
//! stroked paths, `<rect rx>` and userSpaceOnUse `<linearGradient>`. No filters, CSS or masks. The
//! CSS 3D scene of the TypeScript renderer is left out (it is DOM-only).
//!
//! A state can draw more than the body (see effects.rs): ribbons behind and in front of it, and
//! the marks a thinking or blocked bot turns into. Those stay where they are while the body hops
//! and rolls.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use serde_json::json;

use super::color::{Rgb, js_round, mix, parse_hex, to_hex};
use super::effects::{Rect, Rim, body_scale, mark_parts, ribbon_pieces, ribbon_reach, rim_of};
use super::eyes::{EyeSpot, PlainLook, eye_path, plain_look};
use super::motion::{EyeLook, Pose, PoseInput, compute_pose, hash_string, noise};
use super::path::{Bounds, Point, fmt, js_num};
use super::types::{AvatarDefinition, AvatarEyes, RenderOptions};
use super::validate::{
    EYE_H, EYE_W, GAZE_X, GAZE_Y, GazeRange, HAPPY_ARC, analyze_body, eye_centers, gaze_range, validate_avatar,
};
use super::volume::{SliceAxis, Volume, VolumeSlice, build_volume};

pub const DEFAULT_COLOR: &str = "#ff6a1a";
pub const DEFAULT_EYE_COLOR: &str = "#151515";
/// `depth` option value that maps to the natural inferred volume.
pub const DEFAULT_DEPTH: f64 = 6.0;
const MAX_DEPTH: f64 = 14.0;
/// Eyes sit this far above the surface so they never touch the body planes. Turned, an eye drifts
/// lift × sin(angle) from its spot: under half a unit at rest, about one at 45°.
const EYE_LIFT: f64 = 1.5;

struct Prepared {
    def: AvatarDefinition,
    vol: Volume,
    polys: Vec<Vec<Point>>,
    range: GazeRange,
    seed: u32,
    /// Prefix for SVG ids; equal geometry and depth always give equal ids, so repeats on a page are safe.
    id: String,
    /// Stable SVG id per distinct cross-section path, assigned once.
    ids: HashMap<String, String>,
    /// Slices kept at a rendered size, per stack, keyed by stack and rounded size.
    detail: RefCell<HashMap<String, Rc<Vec<bool>>>>,
    /// Where the ribbons' hoops are centred and how far the body reaches from there (see `ribbon_rims`).
    rims: OnceCell<(Rim, Rim)>,
}

impl Prepared {
    fn rims(&self) -> &(Rim, Rim) {
        self.rims.get_or_init(|| ribbon_rims(&self.polys, &self.vol.bounds))
    }
}

thread_local! {
    static CACHE: RefCell<Vec<(String, Rc<Prepared>)>> = const { RefCell::new(Vec::new()) };
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    hi.min(lo.max(v))
}

fn depth_option(depth: Option<f64>) -> f64 {
    let d = clamp(depth.filter(|d| d.is_finite()).unwrap_or(DEFAULT_DEPTH), 0.0, MAX_DEPTH);
    js_round(d * 2.0) / 2.0
}

fn base36(mut n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

pub type RenderResult<T> = Result<T, String>;

/// How far the body reaches from the middle of its box, for a working turn's hoop and for a
/// celebration's cage. The hoops' centre hops and stretches with the body (see `ribbons_follow`)
/// but does not lean or sway with it, so each is measured over the leans its state takes, the body
/// stretched about the centre: a working bot leans 2.4° to 6.6° and squashes to 98%; a celebrating
/// one sways ±1.7° and stretches up to 5.5%. The hoops stand just outside that, so the body never
/// moves through a point where a ribbon passes behind it.
pub fn ribbon_rims(polys: &[Vec<Point>], b: &Bounds) -> (Rim, Rim) {
    let [cx, cy] = pivot_of(b);
    let (bx, by) = ((b.min_x + b.max_x) / 2.0, b.max_y);
    // The body leaned by each of `rolls` (degrees, about its base) and stretched by each of `sys` about the centre.
    let posed = |rolls: &[f64], sys: &[f64]| -> Vec<Vec<Vec<Point>>> {
        let mut out = Vec::new();
        for roll in rolls {
            let r = roll.to_radians();
            let (c, sn) = (r.cos(), r.sin());
            for k in sys {
                out.push(
                    polys
                        .iter()
                        .map(|poly| {
                            poly.iter()
                                .map(|[x, y]| {
                                    let lx = bx + c * (x - bx) - sn * (y - by);
                                    let ly = by + sn * (x - bx) + c * (y - by);
                                    [cx + 1.01 * (lx - cx), cy + k * (ly - cy)]
                                })
                                .collect()
                        })
                        .collect(),
                );
            }
        }
        out
    };
    (rim_of(posed(&[2.4, 4.5, 6.6], &[0.98, 1.0]), cx, cy), rim_of(posed(&[-1.7, 0.0, 1.7], &[0.99, 1.055]), cx, cy))
}

/// The rim moved to where the hoops' centre is in this pose: it hops and stretches with the body
/// (the root motion's lift and scale about the base) but does not lean or sway with it.
pub fn ribbons_follow(rim: &Rim, pose: &Pose, b: &Bounds) -> Rim {
    let (bx, by) = ((b.min_x + b.max_x) / 2.0, b.max_y);
    rim.moved(bx + pose.sx * (rim.cx - bx), by + pose.sy * (rim.cy - by) + pose.lift)
}

fn prepare(definition: &AvatarDefinition, depth: f64) -> RenderResult<Rc<Prepared>> {
    let key = format!("{definition:?}|{depth}");
    if let Some(hit) = CACHE.with(|c| c.borrow().iter().find(|(k, _)| *k == key).map(|(_, p)| p.clone())) {
        return Ok(hit);
    }
    let value = json!({
        "version": definition.version,
        "id": definition.id,
        "name": definition.name,
        "body": { "path": definition.body_path },
        "eyes": { "x": definition.eyes.x, "y": definition.eyes.y, "spacing": definition.eyes.spacing, "scale": definition.eyes.scale },
    });
    let def = validate_avatar(&value).map_err(|errors| format!("Invalid avatar definition: {}", errors.join("; ")))?;
    let body = analyze_body(&def.body_path).map_err(|e| format!("Invalid avatar definition: {}", e.0))?;
    let vol = build_volume(&def, depth / DEFAULT_DEPTH, &body);
    let id = format!("oba-{}", base36(hash_string(&format!("{}|{}|{}", def.id, def.body_path, js_num(depth)))));
    let mut ids: HashMap<String, String> = HashMap::new();
    for part in &vol.parts {
        for s in part.z_slices.iter().chain(part.x_slices.iter()) {
            if !ids.contains_key(&s.d) {
                let n = ids.len();
                ids.insert(s.d.clone(), format!("{id}-{n}"));
            }
        }
    }
    let range = gaze_range(&body.polys, &AvatarEyes { scale: vol.eye_scale, ..def.eyes });
    let prepared = Rc::new(Prepared {
        seed: hash_string(&def.id),
        def,
        vol,
        polys: body.polys,
        range,
        id,
        ids,
        detail: RefCell::new(HashMap::new()),
        rims: OnceCell::new(),
    });
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= 64 {
            c.remove(0);
        }
        c.push((key, prepared.clone()));
    });
    Ok(prepared)
}

/// The validated definition and its cached volume, exactly as the renderers use them. Errs if invalid.
pub fn prepared_volume(definition: &AvatarDefinition, depth: Option<f64>) -> RenderResult<(AvatarDefinition, Volume)> {
    let p = prepare(definition, depth_option(depth))?;
    Ok((p.def.clone(), p.vol.clone()))
}

fn pose_of(p: &Prepared, options: &RenderOptions) -> Pose {
    compute_pose(&PoseInput {
        seed: p.seed,
        time: options.time.filter(|t| t.is_finite()).unwrap_or(0.0),
        state: options.state,
        mood: options.mood,
        motion: options.motion,
        state_age: options.state_age,
        gaze: options.gaze,
        reduced_motion: options.reduced_motion,
        view: options.view,
        blend: options.motion_blend.clone(),
    })
}

fn esc(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '&' | '<' | '>' | '"' | '\'') { format!("&#{};", c as u32) } else { c.to_string() })
        .collect()
}

// ---------------------------------------------------------------------------
// View math

/// 2D affine matrix in SVG order (a b c d e f).
pub type Matrix = [f64; 6];

fn mul(m: &Matrix, n: &Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

fn translate(x: f64, y: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

fn about(x: f64, y: f64, m: &Matrix) -> Matrix {
    mul(&translate(x, y), &mul(m, &translate(-x, -y)))
}

fn invert(m: &Matrix) -> Matrix {
    let [a, b, c, d, e, g] = *m;
    let det = a * d - b * c;
    [d / det, -b / det, -c / det, a / det, (c * g - d * e) / det, (b * e - a * g) / det]
}

fn round4(v: f64) -> f64 {
    js_round(v * 10000.0) / 10000.0
}

fn mat(m: &Matrix) -> String {
    let parts: Vec<String> =
        m.iter().enumerate().map(|(i, v)| if i < 4 { js_num(round4(*v)) } else { fmt(*v) }).collect();
    format!("matrix({})", parts.join(" "))
}

#[derive(Debug, Clone, Copy)]
pub struct ViewBasis {
    pub c_y: f64,
    pub s_y: f64,
    pub c_p: f64,
    pub s_p: f64,
    /// cos(yaw)·cos(pitch): > 0 when the front faces the viewer.
    pub facing: f64,
}

pub fn view_basis(yaw: f64, pitch: f64) -> ViewBasis {
    let (c_y, c_p) = (yaw.cos(), pitch.cos());
    ViewBasis { c_y, s_y: yaw.sin(), c_p, s_p: pitch.sin(), facing: c_y * c_p }
}

/// Minimum facing (toward-viewer component of the surface normal) for an eye to be drawn.
const EYE_FACING: f64 = 0.22;

/// Toward-viewer component of a model-space direction under this view (1 = straight at the viewer).
pub fn facing_of(v: &ViewBasis, [x, y, z]: [f64; 3]) -> f64 {
    v.s_p * y + v.c_p * (-v.s_y * x + v.c_y * z)
}

/// Whether eye `i` is drawn: every surface normal under it must face the viewer.
fn eye_visible(vol: &Volume, v: &ViewBasis, i: usize) -> bool {
    vol.eye_normals[i].iter().all(|n| facing_of(v, *n) > EYE_FACING)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stack {
    Z,
    X,
}

impl Stack {
    fn name(self) -> &'static str {
        match self {
            Stack::Z => "z",
            Stack::X => "x",
        }
    }
}

/// Which slice stack faces the viewer more: front-facing z-planes or side-facing x-planes.
pub fn active_stack(v: &ViewBasis) -> Stack {
    if v.c_y.abs() >= v.s_y.abs() { Stack::Z } else { Stack::X }
}

/// Orthographic projection of the plane at depth z, pivoting on `pivot` at z = 0.
/// Yaw turns about the vertical axis, then pitch tips about the horizontal axis.
pub fn plane_matrix(v: &ViewBasis, z: f64, pivot: Point) -> Matrix {
    let [cx, cy] = pivot;
    [
        v.c_y,
        v.s_y * v.s_p,
        0.0,
        v.c_p,
        cx - v.c_y * cx + v.s_y * z,
        cy - v.c_p * cy - v.s_y * v.s_p * cx - v.c_y * v.s_p * z,
    ]
}

/// Projection of the side plane x = x0, whose local coordinates are (u = -z, y).
pub fn side_plane_matrix(v: &ViewBasis, x0: f64, pivot: Point) -> Matrix {
    let [cx, cy] = pivot;
    [-v.s_y, v.c_y * v.s_p, 0.0, v.c_p, cx + v.c_y * (x0 - cx), cy - v.c_p * cy + v.s_y * v.s_p * (x0 - cx)]
}

/// Project a 3D point: screen x, screen y and depth toward the viewer.
pub fn project_point(v: &ViewBasis, [x, y, z]: [f64; 3], pivot: Point) -> [f64; 3] {
    let m = plane_matrix(v, z, pivot);
    let (px, py) = (x - pivot[0], y - pivot[1]);
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5], v.s_p * py + v.c_p * (-v.s_y * px + v.c_y * z)]
}

fn root_matrix(pose: &Pose, b: &Bounds) -> Matrix {
    let (bx, by) = ((b.min_x + b.max_x) / 2.0, b.max_y);
    let r = pose.roll.to_radians();
    let rot: Matrix = [r.cos(), r.sin(), -r.sin(), r.cos(), 0.0, 0.0];
    mul(&translate(0.0, pose.lift), &about(bx, by, &mul(&rot, &[pose.sx, 0.0, 0.0, pose.sy, 0.0, 0.0])))
}

fn pivot_of(b: &Bounds) -> Point {
    [(b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0]
}

/// The hop and the share of a point's distance from the base that roll and squash can move it.
const POSE_LIFT: f64 = 4.0;
const POSE_SWAY: f64 = 0.2;

/// A square of viewBox units that holds the avatar at every view and pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

/// A square of viewBox units that holds the avatar at every view and pose. Errs if invalid.
pub fn avatar_extent(definition: &AvatarDefinition, depth: Option<f64>) -> RenderResult<Extent> {
    let p = prepare(definition, depth_option(depth))?;
    let vol = &p.vol;
    let b = &vol.bounds;
    let [cx, cy] = pivot_of(b);
    let deepest = vol.parts.iter().map(|part| part.depth).fold(f64::NEG_INFINITY, f64::max);
    let radius = (b.max_x - b.min_x).hypot(b.max_y - b.min_y).hypot(deepest + 2.0 * EYE_LIFT) / 2.0;
    // Ribbons reach their hoop plus their band from the middle of the body's box.
    let (work, cage) = p.rims();
    // The hoops' centre also hops and stretches with the body: as far as the body's own pose moves it.
    let reach = |rim: &Rim| {
        let r = ribbon_reach(Some(rim.max));
        (rim.cx - cx).hypot(rim.cy - cy) + POSE_LIFT + POSE_SWAY * (b.max_y - rim.cy).abs() + r.radius + r.band + 1.2
    };
    let half = (radius + POSE_LIFT + POSE_SWAY * (radius + b.max_y - cy)).max(reach(work)).max(reach(cage));
    Ok(Extent { x: cx - half, y: cy - half, size: 2.0 * half })
}

/// Whether a slice of the active stack shows: z-slices hidden behind the core are skipped.
fn slice_cull(v: &ViewBasis) -> impl Fn(&VolumeSlice) -> bool {
    // Screen shift per unit of depth, and the smallest on-screen size of an in-plane inset.
    let sweep = v.s_y.hypot(v.c_y * v.s_p);
    let inset = v.c_y.abs().min(v.c_p.abs());
    move |s| s.axis != SliceAxis::Z || s.inset <= 0.0 || s.at.abs() * sweep > s.inset * inset
}

/// Closest on-screen spacing, CSS px, between cross-sections a small live avatar keeps.
const SLICE_GAP_PX: f64 = 0.75;

/// Per slice of a stack, in part order: whether it is kept at this rendered size. Within each part,
/// slices closer than SLICE_GAP_PX to the last kept one are dropped; the outermost always stay, and
/// so does the core, which `slice_cull` relies on to cover the inset slices it hides.
fn slice_detail(p: &Prepared, stack: Stack, size: Option<f64>) -> Rc<Vec<bool>> {
    let px = size.filter(|s| s.is_finite() && *s > 0.0).map_or(0.0, js_round);
    let key = format!("{}{}", stack.name(), js_num(px));
    if let Some(keep) = p.detail.borrow().get(&key) {
        return keep.clone();
    }
    let gap = if px > 0.0 { (SLICE_GAP_PX * 100.0) / px } else { 0.0 };
    let mut keep = Vec::new();
    for part in &p.vol.parts {
        let slices = if stack == Stack::Z { &part.z_slices } else { &part.x_slices };
        let mut last = f64::NEG_INFINITY;
        for (j, s) in slices.iter().enumerate() {
            let core = s.axis == SliceAxis::Z && s.at == 0.0 && s.inset == 0.0;
            let kept = core || j == 0 || j == slices.len() - 1 || s.at - last >= gap;
            if kept {
                last = s.at;
            }
            keep.push(kept);
        }
    }
    let keep = Rc::new(keep);
    let mut detail = p.detail.borrow_mut();
    if detail.len() >= 32 {
        detail.clear();
    }
    detail.insert(key, keep.clone());
    keep
}

/// Every cross-section a view draws, with its projection: the stack facing the viewer, minus
/// z-slices hidden behind the core (they add nothing to the silhouette or the paint). Their union
/// is the silhouette on screen. Each is (slice, matrix, part index, slice index).
fn drawn_slices<'a>(p: &'a Prepared, v: &ViewBasis, size: Option<f64>) -> Vec<(&'a VolumeSlice, Matrix, usize, usize)> {
    let vol = &p.vol;
    let stack = active_stack(v);
    let pivot = pivot_of(&vol.bounds);
    let drawn = slice_cull(v);
    let kept = slice_detail(p, stack, size);
    let mut k = 0;
    let mut out = Vec::new();
    for (i, part) in vol.parts.iter().enumerate() {
        let slices = if stack == Stack::Z { &part.z_slices } else { &part.x_slices };
        for (j, s) in slices.iter().enumerate() {
            let keep = kept[k];
            k += 1;
            if !keep || !drawn(s) {
                continue;
            }
            let m = if stack == Stack::Z { plane_matrix(v, s.at, pivot) } else { side_plane_matrix(v, s.at, pivot) };
            out.push((s, m, i, j));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Eyes (shared by every frame, in face-plane coordinates)

struct EyeState {
    /// Neutral center and size.
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    /// Gaze offset.
    dx: f64,
    dy: f64,
    /// Drawn pill height after blink/squint, and its downward shift (lids close toward a point below center).
    open: f64,
    shift: f64,
    look: EyeLook,
    /// The plain pill or the plain happy arc, drawn as such; None for every other look (a path).
    plain: Option<PlainLook>,
}

fn eye_states(p: &Prepared, pose: &Pose) -> [EyeState; 2] {
    let s = p.vol.eye_scale;
    let (w, h) = (EYE_W * s, EYE_H * s);
    let r = &p.range;
    let dx = pose.gaze_x * (if pose.gaze_x >= 0.0 { r.right } else { r.left }) * GAZE_X * s;
    let dy = pose.gaze_y * (if pose.gaze_y >= 0.0 { r.down } else { r.up }) * GAZE_Y * s;
    let centers = eye_centers(&p.def.eyes);
    let plain = plain_look(&pose.eyes);
    let make = |i: usize| {
        let squint = if i == 0 { pose.squint_l } else { pose.squint_r };
        let open = (h * (0.09f64).max((1.0 - pose.blink) * (1.0 - squint * 0.4))).max(w * 0.55);
        EyeState {
            x: centers[i][0],
            y: centers[i][1],
            w,
            h,
            dx,
            dy,
            open,
            shift: (h - open) * 0.18,
            look: pose.eyes,
            plain,
        }
    };
    [make(0), make(1)]
}

/// Where an eye is drawn, with its gaze offset.
fn eye_spot(e: &EyeState) -> EyeSpot {
    EyeSpot { x: e.x + e.dx, y: e.y + e.dy, w: e.w, h: e.h, open: e.open, shift: e.shift }
}

fn happy_arc(e: &EyeState) -> String {
    let (ex, ey) = (e.x + e.dx, e.y + e.dy);
    let hw = e.w * HAPPY_ARC.half_width;
    let base = ey + e.h * HAPPY_ARC.base;
    format!(
        "M{} {}Q{} {} {} {}",
        fmt(ex - hw),
        fmt(base),
        fmt(ex),
        fmt(ey - e.h * HAPPY_ARC.peak),
        fmt(ex + hw),
        fmt(base)
    )
}

/// How one eye is drawn, in its face plane's coordinates (viewBox units): the plain pill, the
/// plain happy arc (stroked, round caps), or for every other look a filled outline (a waiting
/// bot's slits, and every shape on the way from one look to the next).
#[derive(Debug, Clone, PartialEq)]
pub enum EyeMark {
    Pill { x: f64, y: f64, width: f64, height: f64, radius: f64 },
    Arc { d: String, stroke_width: f64 },
    Path { d: String },
}

fn eye_mark(e: &EyeState) -> EyeMark {
    match e.plain {
        Some(PlainLook::Arc) => EyeMark::Arc { d: happy_arc(e), stroke_width: e.w * HAPPY_ARC.stroke },
        None => EyeMark::Path { d: eye_path(&e.look, &eye_spot(e)) },
        Some(PlainLook::Pill) => {
            let (cx, cy) = (e.x + e.dx, e.y + e.dy + e.shift);
            EyeMark::Pill {
                x: cx - e.w / 2.0,
                y: cy - e.open / 2.0,
                width: e.w,
                height: e.open,
                radius: e.w.min(e.open) / 2.0,
            }
        }
    }
}

/// MausBot: a smile under the eyes, stroked like the happy arc. It follows the gaze with the eyes,
/// deepens as the eyes turn happy and flattens as they turn to a waiting slit.
fn mouth_mark(eyes: &[EyeState; 2]) -> EyeMark {
    let [a, b] = eyes;
    let x = (a.x + b.x) / 2.0 + a.dx;
    let y = a.y + a.h * 0.62 + a.dy;
    let hw = ((b.x - a.x) * 0.3).max(a.w * 0.7);
    let happy = clamp(a.look.arc, 0.0, 1.0);
    let flat = clamp(a.look.slit, 0.0, 1.0);
    let depth = hw * (0.42 + 0.38 * happy) * (1.0 - 0.8 * flat);
    EyeMark::Arc {
        d: format!("M{} {}Q{} {} {} {}", fmt(x - hw), fmt(y), fmt(x), fmt(y + 2.0 * depth), fmt(x + hw), fmt(y)),
        stroke_width: a.w * 0.55,
    }
}

fn colors(options: &RenderOptions) -> (String, String) {
    let pick = |value: &Option<String>, default: &str| {
        to_hex(value.as_deref().and_then(parse_hex).or_else(|| parse_hex(default)).unwrap_or([0.0; 3]))
    };
    (pick(&options.color, DEFAULT_COLOR), pick(&options.eye_color, DEFAULT_EYE_COLOR))
}

// ---------------------------------------------------------------------------
// Flat frames: the whole projected avatar as plain data

#[derive(Debug, Clone, PartialEq)]
pub struct RibbonGradient {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub stops: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrameShape {
    Slice {
        /// Stable across frames for the same cross-section: use it as a key in retained renderers.
        key: String,
        /// Id of an entry in `AvatarFrame::paths`.
        path: String,
        matrix: Matrix,
    },
    /// `key` is "eye-0", "eye-1" or "mouth".
    Eye { key: &'static str, matrix: Matrix, mark: EyeMark },
    /// A piece of a ribbon: fill `d` in `fill`. Each ribbon has a hue of its own, set apart from
    /// the body's; a piece `front` of the body is lighter than one behind it.
    Ribbon {
        key: String,
        d: String,
        bbox: Rect,
        matrix: Matrix,
        front: bool,
        /// The ribbon's colour where it is plainest (the middle of its gradient).
        fill: String,
        /// A linear gradient along the whole ribbon, in the same coordinates as `d`: from its tail
        /// (x1, y1) to its head (x2, y2), the stops evenly spaced.
        gradient: RibbonGradient,
    },
}

/// One cross-section outline, shared by every shape that draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct FramePath {
    pub id: String,
    pub d: String,
    pub even_odd: bool,
    pub bbox: Rect,
}

/// One frame of the avatar flattened onto the screen: every cross-section the view draws with its
/// projection, and the visible eyes, sorted back to front, between the ribbons behind the body and
/// those in front of it. The marks of `Thinking` and `Blocked` are listed as slices too, in place
/// of the body. Painting them in order (body paths in `color`, eyes in `ink`) inside the `root`
/// transform and a 0 0 100 100 viewBox gives the avatar exactly as `render_avatar_svg` draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct AvatarFrame {
    pub name: String,
    pub color: String,
    pub ink: String,
    /// Distinct cross-section paths this frame uses; ids are stable for equal geometry and depth.
    pub paths: Vec<FramePath>,
    /// 2D motion (hop, roll, squash) about the base of the body, applied to every shape.
    pub root: Matrix,
    /// Back to front.
    pub shapes: Vec<FrameShape>,
}

/// Compute one flat frame of a definition. Errs if the definition is invalid.
pub fn avatar_frame(definition: &AvatarDefinition, options: &RenderOptions) -> RenderResult<AvatarFrame> {
    flat_frame(definition, options, None)
}

/// `avatar_frame` for a live avatar drawn `options.size` pixels wide: it skips cross-sections that
/// would sit less than a pixel apart. `size` never changes what `avatar_frame` returns.
pub fn live_frame(definition: &AvatarDefinition, options: &RenderOptions) -> RenderResult<AvatarFrame> {
    flat_frame(definition, options, options.size)
}

/// The colour `t` (0..1) of the way along a colour run, in sRGB between its stops.
fn run_at(run: &[Rgb], t: f64) -> String {
    let x = clamp(t, 0.0, 1.0) * (run.len() - 1) as f64;
    let n = (x.floor() as usize).min(run.len() - 2);
    to_hex(mix(run[n], run[n + 1], x - n as f64))
}

/// MausBot: every ribbon takes one of our comet palettes (three neighbouring hues head to tail), whatever the body's colour.
const COMET_COLORS: [[&str; 3]; 6] = [
    ["#A855F7", "#6366F1", "#38BDF8"],
    ["#22D3EE", "#34D399", "#A3E635"],
    ["#FB923C", "#F43F5E", "#D946EF"],
    ["#818CF8", "#C084FC", "#F472B6"],
    ["#FACC15", "#FB923C", "#EC4899"],
    ["#34D399", "#22D3EE", "#60A5FA"],
];

fn comet_palette(seed: u32) -> Vec<Vec<Rgb>> {
    let turn = (noise(seed, 500) * COMET_COLORS.len() as f64).floor() as usize;
    (0..9)
        .map(|i| {
            COMET_COLORS[(i + turn) % COMET_COLORS.len()].iter().map(|hex| parse_hex(hex).unwrap_or([0.0; 3])).collect()
        })
        .collect()
}

struct RibbonFx {
    d: String,
    bbox: Rect,
    front: bool,
    fill: String,
    gradient: RibbonGradient,
}

struct MarkFx {
    id: String,
    d: String,
    bbox: Rect,
    matrix: Matrix,
}

/// What a frame draws besides the body, in viewBox units outside the body's own motion.
struct Effects {
    /// How much of its size the body keeps while it gives way to a mark (0 = gone).
    body: f64,
    ribbons: Vec<RibbonFx>,
    marks: Vec<MarkFx>,
}

fn effects_of(p: &Prepared, pose: &Pose, options: &RenderOptions) -> Effects {
    let b = &p.vol.bounds;
    let time = options.time.filter(|t| t.is_finite()).unwrap_or(0.0);
    let still = options.reduced_motion;
    let [cx, cy] = pivot_of(b);
    let ribbons = if still {
        Vec::new()
    } else {
        let (work, cage) = p.rims();
        let rim = ribbons_follow(if pose.cage >= 0.5 { cage } else { work }, pose, b);
        let palette = comet_palette(p.seed);
        ribbon_pieces(b, pose.ribbons, p.seed, time, pose.sweep, pose.cage, Some(&rim))
            .into_iter()
            .map(|r| {
                let hue = &palette[r.ribbon % 9];
                // The piece's stretch of its ribbon's colour run, front and back alike.
                let [s0, s1] = r.along;
                let stops: Vec<String> = [s0, (s0 + s1) / 2.0, s1].iter().map(|t| run_at(hue, *t)).collect();
                // A gradient needs some length; a piece too short to have one takes its middle colour.
                let long = (r.to[0] - r.from[0]).hypot(r.to[1] - r.from[1]) > 0.01;
                let gradient = if long {
                    RibbonGradient { x1: r.from[0], y1: r.from[1], x2: r.to[0], y2: r.to[1], stops: stops.clone() }
                } else {
                    RibbonGradient {
                        x1: r.from[0],
                        y1: r.from[1],
                        x2: r.from[0] + 0.01,
                        y2: r.from[1],
                        stops: vec![stops[1].clone(); 3],
                    }
                };
                RibbonFx { d: r.d, bbox: r.bbox, front: r.front, fill: stops[1].clone(), gradient }
            })
            .collect()
    };
    let marks = mark_parts(b, pose.bang, pose.dots, p.seed, time, still)
        .into_iter()
        .map(|part| {
            let turn: Matrix = [part.turn.cos(), part.turn.sin(), -part.turn.sin(), part.turn.cos(), 0.0, 0.0];
            MarkFx {
                id: format!("{}-{}", p.id, part.shape),
                d: part.d,
                bbox: part.bbox,
                matrix: mul(
                    &about(cx, cy, &turn),
                    &mul(&translate(part.x, part.y), &[part.scale, 0.0, 0.0, part.scale, 0.0, 0.0]),
                ),
            }
        })
        .collect();
    Effects { body: body_scale(pose.bang, pose.dots), ribbons, marks }
}

fn flat_frame(definition: &AvatarDefinition, options: &RenderOptions, size: Option<f64>) -> RenderResult<AvatarFrame> {
    let p = prepare(definition, depth_option(options.depth))?;
    let vol = &p.vol;
    let b = &vol.bounds;
    let (color, ink) = colors(options);
    let pose = pose_of(&p, options);
    let pivot = pivot_of(b);
    let v = view_basis(pose.yaw, pose.pitch);
    let stack = active_stack(&v);
    let eyes_mid: Point = [p.def.eyes.x, p.def.eyes.y];
    let fx = effects_of(&p, &pose, options);
    let root = root_matrix(&pose, b);
    // The body shrinks toward its center as it gives way to a mark.
    let shrink = |m: Matrix| {
        if fx.body < 1.0 { mul(&about(pivot[0], pivot[1], &[fx.body, 0.0, 0.0, fx.body, 0.0, 0.0]), &m) } else { m }
    };

    // Only the cross-sections this view draws are listed; ids are stable across frames.
    let mut paths: Vec<FramePath> = Vec::new();
    let mut path_at: HashMap<String, usize> = HashMap::new();
    let mut items: Vec<(f64, FrameShape)> = Vec::new();
    if fx.body > 0.0 {
        for (s, m, i, j) in drawn_slices(&p, &v, size) {
            let at = *path_at.entry(s.d.clone()).or_insert_with(|| {
                paths.push(FramePath {
                    id: p.ids.get(&s.d).cloned().unwrap_or_default(),
                    d: s.d.clone(),
                    even_odd: s.even_odd,
                    bbox: Rect { x: s.bbox.x, y: s.bbox.y, w: s.bbox.w, h: s.bbox.h },
                });
                paths.len() - 1
            });
            // Depth keys are taken under the eyes for the eye part, so only depth decides eye-vs-body order there.
            let [rx, ry] = if i == vol.eye_part { eyes_mid } else { vol.parts[i].center };
            let depth = if stack == Stack::Z {
                project_point(&v, [rx, ry, s.at], pivot)[2]
            } else {
                project_point(&v, [s.at, ry, 0.0], pivot)[2]
            };
            items.push((
                depth,
                FrameShape::Slice {
                    key: format!("{i}{}{j}", stack.name()),
                    path: paths[at].id.clone(),
                    matrix: shrink(m),
                },
            ));
        }
        // Painted on the surface; each eye goes once the surface under it turns away.
        let eyes = eye_states(&p, &pose);
        if eye_visible(vol, &v, 0) && eye_visible(vol, &v, 1) {
            // MausBot: the mouth sits on the face plane between the two eyes.
            let z = (vol.eye_z[0] + vol.eye_z[1]) / 2.0 + EYE_LIFT;
            items.push((
                project_point(&v, [eyes_mid[0], eyes_mid[1], z], pivot)[2] + 1e-6,
                FrameShape::Eye { key: "mouth", matrix: shrink(plane_matrix(&v, z, pivot)), mark: mouth_mark(&eyes) },
            ));
        }
        for (idx, e) in eyes.iter().enumerate() {
            if eye_visible(vol, &v, idx) {
                let z = vol.eye_z[idx] + EYE_LIFT;
                items.push((
                    project_point(&v, [eyes_mid[0], eyes_mid[1], z], pivot)[2] + 1e-6,
                    FrameShape::Eye {
                        key: if idx == 0 { "eye-0" } else { "eye-1" },
                        matrix: shrink(plane_matrix(&v, z, pivot)),
                        mark: eye_mark(e),
                    },
                ));
            }
        }
        items.sort_by(|a, c| a.0.total_cmp(&c.0));
    }

    // Ribbons and marks stay put while the body moves: their matrices undo the root's motion.
    let fixed = if fx.ribbons.is_empty() && fx.marks.is_empty() { root } else { invert(&root) };
    let ribbon = |front: bool| -> Vec<FrameShape> {
        fx.ribbons
            .iter()
            .enumerate()
            .filter(|(_, r)| r.front == front)
            .map(|(i, r)| FrameShape::Ribbon {
                key: format!("ribbon-{i}"),
                d: r.d.clone(),
                bbox: r.bbox,
                matrix: fixed,
                front,
                fill: r.fill.clone(),
                gradient: r.gradient.clone(),
            })
            .collect()
    };
    let mut marks: Vec<FrameShape> = Vec::new();
    for (i, mark) in fx.marks.iter().enumerate() {
        if !path_at.contains_key(&mark.id) {
            path_at.insert(mark.id.clone(), paths.len());
            paths.push(FramePath { id: mark.id.clone(), d: mark.d.clone(), even_odd: false, bbox: mark.bbox });
        }
        marks.push(FrameShape::Slice {
            key: format!("mark-{i}"),
            path: mark.id.clone(),
            matrix: mul(&fixed, &mark.matrix),
        });
    }
    let mut shapes = ribbon(false);
    shapes.extend(items.into_iter().map(|(_, s)| s));
    shapes.extend(ribbon(true));
    shapes.extend(marks);
    Ok(AvatarFrame { name: p.def.name.clone(), color, ink, paths, root, shapes })
}

// ---------------------------------------------------------------------------
// Standalone SVG still

fn eye_svg(mark: &EyeMark, ink: &str) -> String {
    match mark {
        EyeMark::Path { d } => format!("<path d=\"{d}\" fill=\"{ink}\"/>"),
        EyeMark::Arc { d, stroke_width } => format!(
            "<path d=\"{d}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
            fmt(*stroke_width)
        ),
        EyeMark::Pill { x, y, width, height, radius } => {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{ink}\"/>",
                fmt(*x),
                fmt(*y),
                fmt(*width),
                fmt(*height),
                fmt(*radius)
            )
        }
    }
}

/// A ribbon piece with its gradient alongside it. The id is taken from the gradient itself, so
/// repeats on a page (the same avatar at the same moment) share one harmlessly.
fn ribbon_svg(d: &str, g: &RibbonGradient) -> String {
    let key = format!("{},{},{},{},{}", fmt(g.x1), fmt(g.y1), fmt(g.x2), fmt(g.y2), g.stops.join(","));
    let id = format!("oba-g{}", base36(hash_string(&key)));
    let n = g.stops.len() - 1;
    let stops: String = g
        .stops
        .iter()
        .enumerate()
        .map(|(i, c)| format!("<stop offset=\"{}\" stop-color=\"{c}\"/>", fmt(i as f64 / n as f64)))
        .collect();
    format!(
        "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{stops}</linearGradient><path d=\"{d}\" fill=\"url(#{id})\"/>",
        fmt(g.x1),
        fmt(g.y1),
        fmt(g.x2),
        fmt(g.y2)
    )
}

/// Render a definition to a standalone SVG string. Errs if the definition is invalid.
pub fn render_avatar_svg(definition: &AvatarDefinition, options: &RenderOptions) -> RenderResult<String> {
    let frame = avatar_frame(definition, options)?;
    let defs: String = frame
        .paths
        .iter()
        .map(|d| {
            format!("<path id=\"{}\" d=\"{}\"{}/>", d.id, d.d, if d.even_odd { " fill-rule=\"evenodd\"" } else { "" })
        })
        .collect();
    let body: String = frame
        .shapes
        .iter()
        .map(|s| match s {
            // QtSvg reads `xlink:href` on `<use>` (plain `href` only in newer Qt), so both are written.
            FrameShape::Slice { path, matrix, .. } => {
                format!("<use href=\"#{path}\" xlink:href=\"#{path}\" transform=\"{}\"/>", mat(matrix))
            }
            FrameShape::Eye { matrix, mark, .. } => {
                format!("<g transform=\"{}\">{}</g>", mat(matrix), eye_svg(mark, &frame.ink))
            }
            FrameShape::Ribbon { d, matrix, gradient, .. } => {
                format!("<g transform=\"{}\">{}</g>", mat(matrix), ribbon_svg(d, gradient))
            }
        })
        .collect();
    let dims = options
        .size
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(|s| fmt(clamp(s, 8.0, 4096.0)))
        .map_or(String::new(), |s| format!(" width=\"{s}\" height=\"{s}\""));
    let label = esc(&frame.name);
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 100 100\"{dims} role=\"img\" aria-label=\"{label}\" overflow=\"visible\"><title>{label}</title><defs>{defs}</defs><g transform=\"{}\" fill=\"{}\">{body}</g></svg>",
        mat(&frame.root),
        frame.color
    ))
}
