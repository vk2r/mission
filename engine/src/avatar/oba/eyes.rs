//! The outline of one eye for any look (ported from `eyes.ts`). The plain pill, the happy arc and
//! the flat slit of a waiting bot are each walked as a closed outline from the top centre,
//! clockwise, at even steps; mixing them point by point gives every shape in between, so a change
//! of state or mood morphs the eyes instead of swapping them. The slit lies inside the room
//! validation keeps for the arc.

use std::f64::consts::PI;

use super::motion::EyeLook;
use super::path::{Point, fmt};
use super::validate::HAPPY_ARC;

/// Where one eye is drawn this frame, in its face plane (viewBox units).
#[derive(Debug, Clone, Copy)]
pub struct EyeSpot {
    /// Centre, gaze included.
    pub x: f64,
    pub y: f64,
    /// Neutral size.
    pub w: f64,
    pub h: f64,
    /// Height of the pill after blink and squint, and how far that moves it down.
    pub open: f64,
    pub shift: f64,
}

/// Points per outline.
const STEPS: usize = 40;
/// The slit: as long as the happy arc is wide, this thick (both as shares of the eye's width), tipped this far.
const SLIT_LENGTH: f64 = 1.9;
const SLIT_THICK: f64 = 0.5;
const SLIT_TIP: f64 = 0.2;

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    hi.min(lo.max(v))
}

/// `count` points at even steps along a closed polyline, the first one at its first point.
fn resample(pts: &[Point], count: usize) -> Vec<Point> {
    let n = pts.len();
    let mut at = vec![0.0];
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let prev = at[i];
        at.push(prev + (b[0] - a[0]).hypot(b[1] - a[1]));
    }
    let total = at[n];
    let mut out = Vec::with_capacity(count);
    let mut i = 0;
    for k in 0..count {
        let s = (k as f64 / count as f64) * total;
        while i < n - 1 && at[i + 1] < s {
            i += 1;
        }
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let u = if at[i + 1] > at[i] { (s - at[i]) / (at[i + 1] - at[i]) } else { 0.0 };
        out.push([a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u]);
    }
    out
}

/// An arc of a circle as points, from angle `from` to `to` (radians, y down), ends included.
fn sweep(cx: f64, cy: f64, r: f64, from: f64, to: f64, steps: usize) -> Vec<Point> {
    (0..=steps)
        .map(|i| {
            let a = from + ((to - from) * i as f64) / steps as f64;
            [cx + a.cos() * r, cy + a.sin() * r]
        })
        .collect()
}

/// The pill: a stadium `w` wide and `h` tall about (cx, cy), from the top centre, clockwise.
fn pill(cx: f64, cy: f64, w: f64, h: f64) -> Vec<Point> {
    let r = w.min(h) / 2.0;
    let fx = w / 2.0 - r;
    let fy = h / 2.0 - r;
    let q = PI / 2.0;
    let mut out = vec![[cx, cy - h / 2.0]];
    out.extend(sweep(cx + fx, cy - fy, r, -q, 0.0, 6));
    out.extend(sweep(cx + fx, cy + fy, r, 0.0, q, 6));
    out.extend(sweep(cx - fx, cy + fy, r, q, 2.0 * q, 6));
    out.extend(sweep(cx - fx, cy - fy, r, 2.0 * q, 3.0 * q, 6));
    out
}

/// The happy arc as an outline: its stroke's two edges and round ends, from the top centre, clockwise.
fn arc(e: &EyeSpot) -> Vec<Point> {
    let hw = e.w * HAPPY_ARC.half_width;
    let base = e.y + e.h * HAPPY_ARC.base;
    let peak = e.y - e.h * HAPPY_ARC.peak;
    let r = (e.w * HAPPY_ARC.stroke) / 2.0;
    // The centre line and its upward normal at s (0 = left end, 1 = right end).
    let at = |s: f64| -> (Point, Point) {
        let m = 1.0 - s;
        let x = e.x + hw * (2.0 * s - 1.0);
        let y = (m * m + s * s) * base + 2.0 * m * s * peak;
        let tx = 2.0 * hw;
        let ty = 2.0 * (2.0 * s - 1.0) * (base - peak);
        let len = tx.hypot(ty);
        ([x, y], [ty / len, -tx / len])
    };
    let edge = |from: f64, to: f64, side: f64| -> Vec<Point> {
        (0..=8)
            .map(|i| {
                let (p, n) = at(from + ((to - from) * i as f64) / 8.0);
                [p[0] + n[0] * r * side, p[1] + n[1] * r * side]
            })
            .collect()
    };
    let end = |s: u8| -> Vec<Point> {
        let (p, n) = at(f64::from(s));
        let up = n[1].atan2(n[0]);
        // Round the end from the upper edge to the lower one, the long way round its tip.
        if s == 1 { sweep(p[0], p[1], r, up, up + PI, 8) } else { sweep(p[0], p[1], r, up + PI, up + 2.0 * PI, 8) }
    };
    let mut out = edge(0.5, 1.0, 1.0);
    out.extend(end(1));
    out.extend(edge(1.0, 0.0, -1.0));
    out.extend(end(0));
    out.extend(edge(0.0, 0.5, 1.0));
    out
}

/// The slit: a thin capsule lying almost flat, both eyes tipped the same way; a blink thins it.
fn slit(e: &EyeSpot) -> Vec<Point> {
    let length = e.w * SLIT_LENGTH;
    let thick = e.w * SLIT_THICK * clamp(e.open / e.h, 0.3, 1.0);
    let (cos, sin) = (SLIT_TIP.cos(), SLIT_TIP.sin());
    pill(0.0, 0.0, length, thick).into_iter().map(|[x, y]| [e.x + x * cos - y * sin, e.y + x * sin + y * cos]).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlainLook {
    Pill,
    Arc,
}

/// Whether a look is exactly the plain pill or exactly the happy arc: those are drawn as before.
pub fn plain_look(look: &EyeLook) -> Option<PlainLook> {
    if look.arc >= 1.0 {
        return Some(PlainLook::Arc);
    }
    if look.arc <= 0.0 && look.slit <= 0.0 { Some(PlainLook::Pill) } else { None }
}

/// The outline of one eye, closed, clockwise from the top: the pill, the arc and the slit mixed by the look.
pub fn eye_outline(look: &EyeLook, e: &EyeSpot) -> Vec<Point> {
    let a = clamp(look.arc, 0.0, 1.0);
    let s = clamp(look.slit, 0.0, 1.0 - a);
    let shapes: [(f64, Vec<Point>); 3] =
        [(1.0 - a - s, pill(e.x, e.y + e.shift, e.w, e.open)), (a, arc(e)), (s, slit(e))];
    let mut out = vec![[0.0, 0.0]; STEPS];
    for (weight, shape) in &shapes {
        if *weight <= 0.0 {
            continue;
        }
        for (i, [x, y]) in resample(shape, STEPS).into_iter().enumerate() {
            out[i][0] += x * weight;
            out[i][1] += y * weight;
        }
    }
    out
}

/// A smooth closed path through an outline: a quadratic per point, joined at the midpoints. Absolute M, Q and Z only.
pub fn outline_path(pts: &[Point]) -> String {
    let n = pts.len();
    let mid = |i: usize| -> Point {
        let a = pts[i % n];
        let b = pts[(i + 1) % n];
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
    };
    let start = mid(n - 1);
    let mut d = format!("M{} {}", fmt(start[0]), fmt(start[1]));
    for (i, p) in pts.iter().enumerate().take(n) {
        let m = mid(i);
        d += &format!("Q{} {} {} {}", fmt(p[0]), fmt(p[1]), fmt(m[0]), fmt(m[1]));
    }
    d + "Z"
}

/// The path of one eye for a look that is neither the plain pill nor the plain arc.
pub fn eye_path(look: &EyeLook, e: &EyeSpot) -> String {
    outline_path(&eye_outline(look, e))
}
