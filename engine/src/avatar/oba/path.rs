//! Restricted SVG path geometry: parser, normalizer and flattener (ported from `path.ts`).
//!
//! Accepts the path command set M L H V C S Q T A Z (absolute and relative)
//! and nothing else. The parsed result is re-serialized from numbers, so the
//! renderer never echoes model-written text into markup.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    M { x: f64, y: f64 },
    L { x: f64, y: f64 },
    C { x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64 },
    Q { x1: f64, y1: f64, x: f64, y: f64 },
    A { rx: f64, ry: f64, rot: f64, large: u8, sweep: u8, x: f64, y: f64 },
    Z,
}

pub type Point = [f64; 2];

pub const MAX_CHARS: usize = 6000;
pub const MAX_SEGMENTS: usize = 400;
pub const MAX_SUBPATHS: usize = 16;
pub const COORD_MIN: f64 = -20.0;
pub const COORD_MAX: f64 = 120.0;

#[derive(Debug, Clone, PartialEq)]
pub struct PathError(pub String);

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PathError {}

fn err<T>(m: impl Into<String>) -> Result<T, PathError> {
    Err(PathError(m.into()))
}

fn arg_count(c: char) -> usize {
    match c {
        'M' | 'L' | 'T' => 2,
        'H' | 'V' => 1,
        'C' => 6,
        'S' | 'Q' => 4,
        'A' => 7,
        _ => 0,
    }
}

#[derive(Clone, Copy)]
enum Token {
    Cmd(char),
    Num(f64),
}

const ALLOWED_CMDS: &str = "MmLlHhVvCcSsQqTtAaZz";

fn is_allowed(c: char) -> bool {
    ALLOWED_CMDS.contains(c) || c.is_ascii_digit() || ".,eE+-".contains(c) || c.is_whitespace()
}

/// Length of the number at the start of `s` (`[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?`), if any.
fn number_len(s: &[u8]) -> Option<usize> {
    let mut i = 0;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        i += 1;
    }
    let digits = |from: usize| s[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    let int = digits(i);
    if int > 0 {
        i += int;
        if i < s.len() && s[i] == b'.' {
            i += 1;
            i += digits(i);
        }
    } else if i < s.len() && s[i] == b'.' && digits(i + 1) > 0 {
        i += 1;
        i += digits(i);
    } else {
        return None;
    }
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        let mut j = i + 1;
        if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
            j += 1;
        }
        let e = digits(j);
        if e > 0 {
            i = j + e;
        }
    }
    Some(i)
}

fn tokenize(d: &str) -> Result<Vec<Token>, PathError> {
    if let Some(bad) = d.chars().find(|c| !is_allowed(*c)) {
        return err(format!("unsupported character {:?} (allowed commands: M L H V C S Q T A Z)", bad.to_string()));
    }
    let bytes = d.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if ALLOWED_CMDS.contains(c) {
            tokens.push(Token::Cmd(c));
            i += 1;
        } else if let Some(n) = number_len(&bytes[i..]) {
            let v: f64 = d[i..i + n].parse().unwrap_or(f64::NAN);
            tokens.push(Token::Num(v));
            i += n;
        } else if c == ',' || c.is_whitespace() {
            i += 1;
        } else {
            let end = (i + 12).min(d.len());
            let near = d.get(i..end).unwrap_or(&d[i..]);
            return err(format!("cannot parse path near {near:?}"));
        }
    }
    Ok(tokens)
}

/// Parse and convert to absolute M/L/C/Q/A/Z segments. Coordinates must stay near the 0..100 box.
pub fn parse_path(d: &str) -> Result<Vec<Segment>, PathError> {
    let out = parse_path_unbounded(d)?;
    for s in &out {
        let vals: &[f64] = &match *s {
            Segment::M { x, y } | Segment::L { x, y } => vec![x, y],
            Segment::C { x1, y1, x2, y2, x, y } => vec![x1, y1, x2, y2, x, y],
            Segment::Q { x1, y1, x, y } => vec![x1, y1, x, y],
            Segment::A { rx, ry, x, y, .. } => vec![rx, ry, x, y],
            Segment::Z => vec![],
        };
        for v in vals {
            if *v < COORD_MIN || *v > COORD_MAX {
                return err(format!("coordinate {} far outside the 0..100 viewBox", js_num(*v)));
            }
        }
    }
    Ok(out)
}

/// `parse_path` without the check that coordinates sit near the 0..100 box: for rendered paths,
/// which are posed and may reach past it.
pub fn parse_path_unbounded(d: &str) -> Result<Vec<Segment>, PathError> {
    if d.trim().is_empty() {
        return err("path is empty");
    }
    if d.encode_utf16().count() > MAX_CHARS {
        return err(format!("path longer than {MAX_CHARS} characters"));
    }
    let tokens = tokenize(d)?;
    let mut out: Vec<Segment> = Vec::new();
    let mut i = 0;
    let (mut cx, mut cy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
    let mut prev_c: Option<[f64; 2]> = None;
    let mut prev_q: Option<[f64; 2]> = None;
    let mut cmd = '\0';
    let mut subpaths = 0;
    let mut open = false;

    while i < tokens.len() {
        match tokens[i] {
            Token::Cmd(c) => {
                cmd = c;
                i += 1;
                if cmd == 'Z' || cmd == 'z' {
                    if !open {
                        return err("Z without an open subpath");
                    }
                    out.push(Segment::Z);
                    cx = sx;
                    cy = sy;
                    prev_c = None;
                    prev_q = None;
                    open = false;
                    continue;
                }
            }
            Token::Num(_) => {
                if cmd == '\0' {
                    return err("path must start with M");
                } else if cmd == 'Z' || cmd == 'z' {
                    return err("numbers after Z");
                }
            }
        }
        let up = cmd.to_ascii_uppercase();
        let rel = cmd != up;
        if out.is_empty() && up != 'M' {
            return err("path must start with M");
        }
        let n = arg_count(up);
        let mut args = [0.0f64; 7];
        for (k, slot) in args.iter_mut().enumerate().take(n) {
            match tokens.get(i + k) {
                Some(Token::Num(v)) => {
                    if !v.is_finite() {
                        return err("non-finite number");
                    }
                    *slot = *v;
                }
                _ => return err(format!("command {cmd} expects {n} numbers")),
            }
        }
        i += n;
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        let mut next_c = None;
        let mut next_q = None;
        match up {
            'M' => {
                if open {
                    out.push(Segment::Z); // implicit close keeps fill semantics explicit
                }
                cx = args[0] + ox;
                cy = args[1] + oy;
                sx = cx;
                sy = cy;
                out.push(Segment::M { x: cx, y: cy });
                subpaths += 1;
                open = true;
                cmd = if rel { 'l' } else { 'L' }; // subsequent pairs are implicit lineto
            }
            'L' => {
                cx = args[0] + ox;
                cy = args[1] + oy;
                out.push(Segment::L { x: cx, y: cy });
            }
            'H' => {
                cx = args[0] + ox;
                out.push(Segment::L { x: cx, y: cy });
            }
            'V' => {
                cy = args[0] + oy;
                out.push(Segment::L { x: cx, y: cy });
            }
            'C' => {
                let (x2, y2) = (args[2] + ox, args[3] + oy);
                out.push(Segment::C { x1: args[0] + ox, y1: args[1] + oy, x2, y2, x: args[4] + ox, y: args[5] + oy });
                next_c = Some([x2, y2]);
                cx = args[4] + ox;
                cy = args[5] + oy;
            }
            'S' => {
                let (x1, y1) = match prev_c {
                    Some(p) => (2.0 * cx - p[0], 2.0 * cy - p[1]),
                    None => (cx, cy),
                };
                let (x2, y2) = (args[0] + ox, args[1] + oy);
                out.push(Segment::C { x1, y1, x2, y2, x: args[2] + ox, y: args[3] + oy });
                next_c = Some([x2, y2]);
                cx = args[2] + ox;
                cy = args[3] + oy;
            }
            'Q' => {
                let (x1, y1) = (args[0] + ox, args[1] + oy);
                out.push(Segment::Q { x1, y1, x: args[2] + ox, y: args[3] + oy });
                next_q = Some([x1, y1]);
                cx = args[2] + ox;
                cy = args[3] + oy;
            }
            'T' => {
                let (x1, y1) = match prev_q {
                    Some(p) => (2.0 * cx - p[0], 2.0 * cy - p[1]),
                    None => (cx, cy),
                };
                out.push(Segment::Q { x1, y1, x: args[0] + ox, y: args[1] + oy });
                next_q = Some([x1, y1]);
                cx = args[0] + ox;
                cy = args[1] + oy;
            }
            'A' => {
                let (rx, ry, rot, la, sw) = (args[0], args[1], args[2], args[3], args[4]);
                if rx < 0.0 || ry < 0.0 {
                    return err("arc radii must be non-negative");
                }
                if (la != 0.0 && la != 1.0) || (sw != 0.0 && sw != 1.0) {
                    return err("arc flags must be 0 or 1, written as separate numbers");
                }
                cx = args[5] + ox;
                cy = args[6] + oy;
                out.push(Segment::A { rx, ry, rot, large: la as u8, sweep: sw as u8, x: cx, y: cy });
            }
            _ => return err(format!("unsupported command {cmd}")),
        }
        prev_c = next_c;
        prev_q = next_q;
        if out.len() > MAX_SEGMENTS {
            return err(format!("more than {MAX_SEGMENTS} segments"));
        }
    }
    if open {
        out.push(Segment::Z);
    }
    if subpaths > MAX_SUBPATHS {
        return err(format!("more than {MAX_SUBPATHS} subpaths"));
    }
    Ok(out)
}

/// A number as JS `String(n)` would print it for the ranges used here.
pub fn js_num(n: f64) -> String {
    if n == 0.0 {
        return "0".to_string();
    }
    format!("{n}")
}

/// Round to 2 decimals (JS `Math.round`) and print, never "-0".
pub fn fmt(n: f64) -> String {
    let r = ((n * 100.0) + 0.5).floor() / 100.0;
    js_num(if r == 0.0 { 0.0 } else { r })
}

/// Compact absolute serialization built only from parsed numbers.
pub fn serialize_path(segs: &[Segment]) -> String {
    let mut out = String::new();
    for s in segs {
        match *s {
            Segment::M { x, y } => out += &format!("M{} {}", fmt(x), fmt(y)),
            Segment::L { x, y } => out += &format!("L{} {}", fmt(x), fmt(y)),
            Segment::C { x1, y1, x2, y2, x, y } => {
                out += &format!("C{} {} {} {} {} {}", fmt(x1), fmt(y1), fmt(x2), fmt(y2), fmt(x), fmt(y))
            }
            Segment::Q { x1, y1, x, y } => out += &format!("Q{} {} {} {}", fmt(x1), fmt(y1), fmt(x), fmt(y)),
            Segment::A { rx, ry, rot, large, sweep, x, y } => {
                out += &format!("A{} {} {} {} {} {} {}", fmt(rx), fmt(ry), fmt(rot), large, sweep, fmt(x), fmt(y))
            }
            Segment::Z => out.push('Z'),
        }
    }
    out
}

/// Largest gap, viewBox units, between a curve and the polygon that stands for it. Volume slices
/// follow that polygon while the flat core draws the real curve, so coarser chords show as facets
/// and teeth along the rim of large avatars.
pub const FLATNESS: f64 = 0.02;

/// Endpoint arc → sampled points (SVG implementation notes, F.6.5).
#[allow(clippy::too_many_arguments)]
fn sample_arc(
    x0: f64,
    y0: f64,
    rx: f64,
    ry: f64,
    rot: f64,
    large: u8,
    sweep: u8,
    x: f64,
    y: f64,
    out: &mut Vec<Point>,
) {
    let (mut rx, mut ry) = (rx, ry);
    if rx == 0.0 || ry == 0.0 || (x0 == x && y0 == y) {
        out.push([x, y]);
        return;
    }
    let phi = rot * PI / 180.0;
    let (cos, sin) = (phi.cos(), phi.sin());
    let (dx, dy) = ((x0 - x) / 2.0, (y0 - y) / 2.0);
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    rx = rx.abs();
    ry = ry.abs();
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        rx *= k;
        ry *= k;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cxp = (coef * rx * y1p) / ry;
    let cyp = (-coef * ry * x1p) / rx;
    let cx = cos * cxp - sin * cyp + (x0 + x) / 2.0;
    let cy = sin * cxp + cos * cyp + (y0 + y) / 2.0;
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let t1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dt = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    if sweep == 0 && dt > 0.0 {
        dt -= 2.0 * PI;
    }
    if sweep != 0 && dt < 0.0 {
        dt += 2.0 * PI;
    }
    // Chords within FLATNESS of the arc: a chord over angle θ sags r(1 - cos(θ/2)).
    let step = 2.0 * (1.0 - FLATNESS / rx.max(ry)).clamp(-1.0, 1.0).acos();
    let steps = ((dt.abs() / step).ceil()).clamp(4.0, 256.0) as usize;
    for k in 1..=steps {
        let t = t1 + (dt * k as f64) / steps as f64;
        let (ex, ey) = (rx * t.cos(), ry * t.sin());
        out.push([cos * ex - sin * ey + cx, sin * ex + cos * ey + cy]);
    }
}

/// Flatten into closed polygons, one per subpath.
pub fn flatten_path(segs: &[Segment]) -> Vec<Vec<Point>> {
    let mut polys: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let (mut px, mut py) = (0.0, 0.0);
    for s in segs {
        match *s {
            Segment::M { x, y } => {
                if !cur.is_empty() {
                    polys.push(std::mem::take(&mut cur));
                }
                cur = vec![[x, y]];
                px = x;
                py = y;
            }
            Segment::L { x, y } => {
                cur.push([x, y]);
                px = x;
                py = y;
            }
            Segment::C { x1, y1, x2, y2, x, y } => {
                // Wang's bound: this many chords keep within FLATNESS of the curve.
                let m =
                    (px - 2.0 * x1 + x2).hypot(py - 2.0 * y1 + y2).max((x1 - 2.0 * x2 + x).hypot(y1 - 2.0 * y2 + y));
                let n = ((0.75 * m / FLATNESS).sqrt().ceil()).clamp(2.0, 128.0) as usize;
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let u = 1.0 - t;
                    cur.push([
                        u * u * u * px + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x,
                        u * u * u * py + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y,
                    ]);
                }
                px = x;
                py = y;
            }
            Segment::Q { x1, y1, x, y } => {
                let n = ((0.25 * (px - 2.0 * x1 + x).hypot(py - 2.0 * y1 + y) / FLATNESS).sqrt().ceil())
                    .clamp(2.0, 128.0) as usize;
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let u = 1.0 - t;
                    cur.push([u * u * px + 2.0 * u * t * x1 + t * t * x, u * u * py + 2.0 * u * t * y1 + t * t * y]);
                }
                px = x;
                py = y;
            }
            Segment::A { rx, ry, rot, large, sweep, x, y } => {
                sample_arc(px, py, rx, ry, rot, large, sweep, x, y, &mut cur);
                px = x;
                py = y;
            }
            Segment::Z => {
                if !cur.is_empty() {
                    px = cur[0][0];
                    py = cur[0][1];
                    polys.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if !cur.is_empty() {
        polys.push(cur);
    }
    polys.retain(|p| p.len() >= 3);
    polys
}

/// Nonzero winding number of a point against all subpaths (SVG default fill rule).
pub fn winding(polys: &[Vec<Point>], x: f64, y: f64) -> i32 {
    let mut w = 0;
    for poly in polys {
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let [xi, yi] = poly[i];
            let [xj, yj] = poly[j];
            if yj <= y {
                if yi > y && (xi - xj) * (y - yj) - (x - xj) * (yi - yj) > 0.0 {
                    w += 1;
                }
            } else if yi <= y && (xi - xj) * (y - yj) - (x - xj) * (yi - yj) < 0.0 {
                w -= 1;
            }
            j = i;
        }
    }
    w
}

pub fn is_inside(polys: &[Vec<Point>], x: f64, y: f64) -> bool {
    winding(polys, x, y) != 0
}

/// Nonzero inside flags for many points on one row (xs ascending), from a
/// single pass over the edges instead of one winding test per point.
pub fn inside_row(polys: &[Vec<Point>], y: f64, xs: &[f64], out: &mut [u8]) {
    let mut hits: Vec<(f64, i32)> = Vec::new();
    for poly in polys {
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let [xj, yj] = poly[j];
            let [xi, yi] = poly[i];
            if yj <= y && yi > y {
                hits.push((xj + ((y - yj) * (xi - xj)) / (yi - yj), 1));
            } else if yi <= y && yj > y {
                hits.push((xj + ((y - yj) * (xi - xj)) / (yi - yj), -1));
            }
            j = i;
        }
    }
    hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut w, mut h) = (0, 0);
    for (k, x) in xs.iter().enumerate() {
        while h < hits.len() && hits[h].0 < *x {
            w += hits[h].1;
            h += 1;
        }
        out[k] = u8::from(w != 0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

pub fn bounds(polys: &[Vec<Point>]) -> Bounds {
    let mut b =
        Bounds { min_x: f64::INFINITY, min_y: f64::INFINITY, max_x: f64::NEG_INFINITY, max_y: f64::NEG_INFINITY };
    for p in polys {
        for [x, y] in p {
            b.min_x = b.min_x.min(*x);
            b.min_y = b.min_y.min(*y);
            b.max_x = b.max_x.max(*x);
            b.max_y = b.max_y.max(*y);
        }
    }
    b
}

/// Filled area estimated on a 1-unit sample grid (nonzero rule).
pub fn filled_area(polys: &[Vec<Point>], b: Bounds) -> usize {
    let mut n = 0;
    let mut xs = Vec::new();
    let mut x = b.min_x.floor() + 0.5;
    while x < b.max_x {
        xs.push(x);
        x += 1.0;
    }
    let mut row = vec![0u8; xs.len()];
    let mut y = b.min_y.floor() + 0.5;
    while y < b.max_y {
        inside_row(polys, y, &xs, &mut row);
        n += row.iter().map(|v| usize::from(*v)).sum::<usize>();
        y += 1.0;
    }
    n
}

/// Split normalized segments into one segment list per subpath.
pub fn split_subpaths(segs: &[Segment]) -> Vec<Vec<Segment>> {
    let mut out: Vec<Vec<Segment>> = Vec::new();
    for s in segs {
        if matches!(s, Segment::M { .. }) {
            out.push(Vec::new());
        }
        if let Some(last) = out.last_mut() {
            last.push(*s);
        }
    }
    out
}
