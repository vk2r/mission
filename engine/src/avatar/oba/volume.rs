//! Implied volume for a flat silhouette (ported from `volume.ts`).
//!
//! The definition stays a 2D path. Here every filled subpath becomes a "part"
//! with a front-surface height field h(x, y) (the back mirrors it about z = 0):
//!
//! - inflate: a rounded slab. Height follows an elliptical edge fillet of the
//!   distance to the outline, so sides curve and the face is flat.
//! - revolve-y: a solid of revolution about the part's vertical center line
//!   (mug body, pot, bulb), taken from exact scanline widths.
//! - revolve-x: a solid of revolution about a horizontal axis shared by the
//!   whole body (barbell: bar → cylinder, plates → discs).
//!
//! From the height field the part is cut, once, into two stacks of plain
//! cross-section paths: z-slices (planes facing the viewer, level sets of h)
//! and x-slices (planes facing sideways, column profiles of h). Rendered in a
//! single flat color, the union of a stack is the silhouette of the solid;
//! the renderer shows whichever stack faces the viewer more, so no plane is
//! ever seen edge-on. Everything here is pure and cached by the renderer.

use std::collections::HashMap;

use super::color::js_round;
use super::contour::{Field, contour_loops, loops_to_path, simplify_loop};
use super::path::{Bounds, Point, bounds, inside_row, is_inside};
use super::types::{AvatarDefinition, AvatarEyes};
use super::validate::{
    BodyAnalysis, EYE_H, EYE_W, GAZE_X, GAZE_Y, GazeRange, analyze_body, drawn_eyes_fit, eye_centers, eye_clearance,
    gaze_range,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    Inflate,
    RevolveX,
    RevolveY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceAxis {
    Z,
    X,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliceBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone)]
pub struct VolumeSlice {
    /// `Z`: the plane z = at, path in (x, y). `X`: the plane x = at, path in (u = -z, y).
    pub axis: SliceAxis,
    pub at: f64,
    pub d: String,
    /// The same cross-section as polygons in its plane's coordinates (`d` is a rounded serialization of them).
    pub loops: Vec<Vec<Point>>,
    /// Contour paths use evenodd; the core slice reuses the original nonzero outline.
    pub even_odd: bool,
    /// Path bounds in its plane's coordinates.
    pub bbox: SliceBox,
    /// z-slices: smallest distance from this cross-section to the outline (0 for the core and walls).
    pub inset: f64,
}

#[derive(Debug, Clone)]
struct Profile {
    start: f64,
    step: f64,
    r: Vec<f64>,
}

/// How a part's front-surface height is computed.
#[derive(Debug, Clone)]
enum HeightModel {
    Inflate,
    RevolveX { profile: Profile, axis: f64, v: f64 },
    RevolveY { profile: Profile, axis: f64, v: f64 },
}

#[derive(Debug, Clone)]
pub struct VolumePart {
    pub kind: PartKind,
    /// Outline plus the holes that cut it, serialized.
    pub d: String,
    pub polys: Vec<Vec<Point>>,
    pub bounds: Bounds,
    pub center: Point,
    /// Twice the largest inscribed radius (inflate) or the largest diameter (revolved).
    pub thickness: f64,
    /// Front-to-back extent, centered on z = 0.
    pub depth: f64,
    /// Inflate only: elliptical edge fillet radii (in-plane and in depth).
    pub rxy: f64,
    pub rz: f64,
    /// Revolved parts: the axis (y for revolve-x, x for revolve-y).
    pub axis: Option<f64>,
    pub z_slices: Vec<VolumeSlice>,
    pub x_slices: Vec<VolumeSlice>,
    model: HeightModel,
}

pub type Vec3 = [f64; 3];

impl VolumePart {
    /// Front surface height at a point, 0 outside the part.
    pub fn height(&self, x: f64, y: f64) -> f64 {
        if !is_inside(&self.polys, x, y) {
            return 0.0;
        }
        let sd = if self.kind == PartKind::Inflate { edge_distance(&self.polys, x, y) } else { 0.0 };
        self.height_of(x, y, sd)
    }

    fn height_of(&self, x: f64, y: f64, sd: f64) -> f64 {
        match &self.model {
            HeightModel::Inflate => surface_z(self.depth, self.rxy, self.rz, sd),
            HeightModel::RevolveX { profile, axis, v } => {
                v * (radius_at(profile, x).powi(2) - (y - axis).powi(2)).max(0.0).sqrt()
            }
            HeightModel::RevolveY { profile, axis, v } => {
                v * (radius_at(profile, y).powi(2) - (x - axis).powi(2)).max(0.0).sqrt()
            }
        }
    }

    /// A level's cross-section as a field: positive inside, zero on the edge, roughly a distance.
    fn level_field(&self, x: f64, y: f64, sd: f64, level: f64) -> f64 {
        match &self.model {
            // Height grows with distance from the outline, so a level is reached at a fixed inset.
            HeightModel::Inflate => sd - inset_at(self.depth, self.rxy, self.rz, level),
            // Height ≥ level where t² ≤ R² - (level / v)²: that difference over 2R is about a distance.
            HeightModel::RevolveX { profile, axis, v } => {
                let r = radius_at(profile, x);
                let t = y - axis;
                (r * r - t * t - (level / v.max(1e-6)).powi(2)) / (2.0 * r.max(0.5))
            }
            HeightModel::RevolveY { profile, axis, v } => {
                let r = radius_at(profile, y);
                let t = x - axis;
                (r * r - t * t - (level / v.max(1e-6)).powi(2)) / (2.0 * r.max(0.5))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Volume {
    pub parts: Vec<VolumePart>,
    pub bounds: Bounds,
    /// Index of the part carrying the eyes.
    pub eye_part: usize,
    /// Depth of each eye's plane: the highest surface under the eye and its gaze travel.
    pub eye_z: [f64; 2],
    /// Drawn eye scale: the definition's scale enlarged as far as the body allows (see EYE_BOOST).
    pub eye_scale: f64,
    /// Host surface normals under each eye (left edge, center, right edge). On a
    /// curved host an eye is hidden once any of them turns away, so a flat eye
    /// never hangs past the curved silhouette.
    pub eye_normals: [Vec<Vec3>; 2],
}

/// Largest depth an inflated part may take, viewBox units.
pub const MAX_PART_DEPTH: f64 = 30.0;
/// Drawn eyes are up to this much larger than the contract's 4 × 11 pill when the body has room.
pub const EYE_BOOST: f64 = 1.45;
/// Gap every drawn eye shape (pill and happy arc) keeps from the outline at rest
/// before the renderer enlarges it, viewBox units. Below this the eyes stay at
/// the definition's own scale.
pub const EYE_BOOST_PAD: f64 = 3.0;
/// Design target for generated avatars: the enlarged eyes keep this much
/// clearance at rest, which also leaves full gaze travel with EYE_CLEARANCE.
/// Advisory (validate.sh --strict); v1 imports below it still render.
pub const FACE_ROOM_TARGET: f64 = 4.5;
const MAX_X_SLICES: f64 = 40.0;
/// Thin regions get refinement slices (see `thin_boxes`): where no inscribed circle
/// within THIN_REACH units has a radius of THIN or more. A thin, deep protrusion
/// seen at an angle shifts more between neighboring slices than it is wide, so
/// without them it fans out into separate copies.
const THIN: f64 = 4.0;
const THIN_REACH: f64 = 5.0;
/// How far a refinement box reaches past its thin region into the body, units.
const THIN_MARGIN: f64 = 2.0;
/// Depth spacing of refinement slices, units.
const FINE_Z: f64 = 0.6;

fn seg_dist2(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len = dx * dx + dy * dy;
    let t = if len > 0.0 { ((px - ax) * dx + (py - ay) * dy) / len } else { 0.0 };
    let t = t.clamp(0.0, 1.0);
    let ex = ax + t * dx - px;
    let ey = ay + t * dy - py;
    ex * ex + ey * ey
}

fn seg_dist(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    seg_dist2(px, py, ax, ay, bx, by).sqrt()
}

/// Distance from a point to the nearest edge of the polygons.
pub fn edge_distance(polys: &[Vec<Point>], x: f64, y: f64) -> f64 {
    let mut best = f64::INFINITY;
    for poly in polys {
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let d = seg_dist(x, y, poly[j][0], poly[j][1], poly[i][0], poly[i][1]);
            if d < best {
                best = d;
            }
            j = i;
        }
    }
    best
}

/// Inward distance to the outline, or 0 outside the fill.
pub fn inside_distance(polys: &[Vec<Point>], x: f64, y: f64) -> f64 {
    if is_inside(polys, x, y) { edge_distance(polys, x, y) } else { 0.0 }
}

/// Outline erosion of an inflated part at depth z (elliptical fillet).
pub fn erosion_at(depth: f64, rxy: f64, rz: f64, z: f64) -> f64 {
    let half = depth / 2.0;
    if rz <= 0.0 {
        return 0.0;
    }
    let u = (z.abs() - (half - rz)) / rz;
    if u <= 0.0 {
        return 0.0;
    }
    rxy * (1.0 - (1.0 - u.min(1.0).powi(2)).max(0.0).sqrt())
}

/// How far inside the outline an inflated part's surface reaches `level` (inverse of `surface_z`).
pub fn inset_at(depth: f64, rxy: f64, rz: f64, level: f64) -> f64 {
    let half = depth / 2.0;
    if rxy <= 0.0 || rz <= 0.0 || level <= half - rz {
        return 0.0;
    }
    if level >= half {
        return rxy;
    }
    let s = (level - (half - rz)) / rz;
    rxy * (1.0 - (1.0 - s * s).max(0.0).sqrt())
}

/// Front surface depth of an inflated part at a point `dist` units inside the outline.
pub fn surface_z(depth: f64, rxy: f64, rz: f64, dist: f64) -> f64 {
    let half = depth / 2.0;
    if rxy <= 0.0 || dist >= rxy {
        return half;
    }
    let k = 1.0 - dist.max(0.0) / rxy;
    half - rz + (1.0 - k * k).max(0.0).sqrt() * rz
}

/// Depth heuristic for inflated parts: as deep as the part is thick, but tall
/// slender parts get up to 2.4x their thickness so they read as discs.
pub fn part_depth(thickness: f64, b: &Bounds, volume: f64) -> f64 {
    let long = (b.max_x - b.min_x).max(b.max_y - b.min_y);
    let raw = 0.8 * thickness.max((0.4 * long).min(2.4 * thickness));
    MAX_PART_DEPTH.min(raw.max(3.0)) * volume
}

/// Sorted crossings of the line (y = s if `rows`, else x = s) with the polygons.
fn crossings(polys: &[Vec<Point>], rows: bool, s: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let (a, b) = if rows { (1, 0) } else { (0, 1) };
    for poly in polys {
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let p = poly[j];
            let q = poly[i];
            if (p[a] <= s && q[a] > s) || (q[a] <= s && p[a] > s) {
                out.push(p[b] + ((s - p[a]) * (q[b] - p[b])) / (q[a] - p[a]));
            }
            j = i;
        }
    }
    out.sort_by(|m, n| m.total_cmp(n));
    out
}

/// Half-widths about `center` along rows (or columns) if every line crosses the
/// part as one interval centered on it; otherwise None (not a solid of revolution).
fn revolve_profile(polys: &[Vec<Point>], rows: bool, lo: f64, hi: f64, center: f64) -> Option<Profile> {
    let step = 0.2;
    let n = (((hi - lo) / step).ceil() + 1.0).max(2.0) as usize;
    let mut r = vec![0.0; n];
    let mut span = 0.0f64;
    let mut mids: Vec<f64> = Vec::new();
    for (k, slot) in r.iter_mut().enumerate() {
        let s = (hi - 0.01).min((lo + 0.01).max(lo + k as f64 * step));
        let xs = crossings(polys, rows, s);
        if xs.is_empty() {
            continue;
        }
        if xs.len() != 2 {
            return None;
        }
        *slot = (xs[1] - xs[0]) / 2.0;
        span = span.max(xs[1] - xs[0]);
        mids.push((xs[0] + xs[1]) / 2.0);
    }
    let tol = 0.8f64.max(0.04 * span);
    if mids.is_empty() || mids.iter().any(|m| (m - center).abs() > tol) {
        return None;
    }
    Some(Profile { start: lo, step, r })
}

fn radius_at(p: &Profile, s: f64) -> f64 {
    let t = (s - p.start) / p.step;
    if t < 0.0 || t > (p.r.len() - 1) as f64 {
        return 0.0;
    }
    let k = ((p.r.len() - 2) as f64).min(t.floor()) as usize;
    p.r[k] + (p.r[k + 1] - p.r[k]) * (t - k as f64)
}

/// Horizontal axis of revolution, if the body looks like a turned object seen
/// from the side: at least two parts, wider than tall, mirror-symmetric about
/// y = axis and with every part centered on it.
pub fn revolve_axis(body: &BodyAnalysis) -> Option<f64> {
    let b = body.bounds;
    let outlines: Vec<_> = body.subpaths.iter().filter(|s| !s.hole).collect();
    if outlines.len() < 2 || b.max_x - b.min_x < 1.1 * (b.max_y - b.min_y) {
        return None;
    }
    let axis = (b.min_y + b.max_y) / 2.0;
    for o in &outlines {
        let ob = bounds(&o.polys);
        if ((ob.min_y + ob.max_y) / 2.0 - axis).abs() > 1.5 {
            return None;
        }
    }
    let mut filled = 0usize;
    let mut off = 0usize;
    let mut xs = Vec::new();
    let mut x = b.min_x + 0.5;
    while x < b.max_x {
        xs.push(x);
        x += 1.0;
    }
    let mut top = vec![0u8; xs.len()];
    let mut bottom = vec![0u8; xs.len()];
    let mut y = b.min_y + 0.5;
    while y < axis {
        inside_row(&body.polys, y, &xs, &mut top);
        inside_row(&body.polys, 2.0 * axis - y, &xs, &mut bottom);
        for k in 0..xs.len() {
            if top[k] != 0 || bottom[k] != 0 {
                filled += 1;
            }
            if top[k] != bottom[k] {
                off += 1;
            }
        }
        y += 1.0;
    }
    if filled > 0 && (off as f64) / (filled as f64) < 0.02 { Some(axis) } else { None }
}

/// Sampled signed distance (positive inside) and height over a part's bounds.
struct Grid {
    x0: f64,
    y0: f64,
    step: f64,
    nx: usize,
    ny: usize,
    sd: Vec<f64>,
    h: Vec<f64>,
}

/// Edges bucketed on a coarse grid, for nearest-edge queries that only visit nearby buckets.
struct EdgeIndex {
    cell: f64,
    buckets: HashMap<i64, Vec<usize>>,
    e: Vec<f64>,
    ox: f64,
    oy: f64,
    span: i64,
}

impl EdgeIndex {
    fn new(polys: &[Vec<Point>], b: &Bounds) -> Self {
        let cell = 6.0;
        let span = (((b.max_x - b.min_x).max(b.max_y - b.min_y) + 8.0) / cell).ceil() as i64 + 1;
        let mut idx =
            EdgeIndex { cell, buckets: HashMap::new(), e: Vec::new(), ox: b.min_x - 4.0, oy: b.min_y - 4.0, span };
        for poly in polys {
            let mut j = poly.len() - 1;
            for i in 0..poly.len() {
                let k = idx.e.len();
                idx.e.extend_from_slice(&[poly[j][0], poly[j][1], poly[i][0], poly[i][1]]);
                let (x0, x1) = (poly[j][0].min(poly[i][0]), poly[j][0].max(poly[i][0]));
                let (y0, y1) = (poly[j][1].min(poly[i][1]), poly[j][1].max(poly[i][1]));
                for cy in idx.cy(y0)..=idx.cy(y1) {
                    for cx in idx.cx(x0)..=idx.cx(x1) {
                        idx.buckets.entry(cy * idx.span + cx).or_default().push(k);
                    }
                }
                j = i;
            }
        }
        idx
    }

    fn cx(&self, x: f64) -> i64 {
        (((x - self.ox) / self.cell).floor() as i64).clamp(0, self.span - 1)
    }

    fn cy(&self, y: f64) -> i64 {
        (((y - self.oy) / self.cell).floor() as i64).clamp(0, self.span - 1)
    }

    /// Distance to the nearest edge, searching rings of buckets outward until no closer edge can exist.
    fn distance(&self, x: f64, y: f64) -> f64 {
        let (bx, by) = (self.cx(x), self.cy(y));
        let mut best = f64::INFINITY;
        let e = &self.e;
        for r in 0..self.span {
            for cy in by - r..=by + r {
                if cy < 0 || cy >= self.span {
                    continue;
                }
                let edge_row = cy == by - r || cy == by + r;
                let stride = if edge_row || r == 0 { 1 } else { 2 * r };
                let mut cx = bx - r;
                while cx <= bx + r {
                    if cx >= 0
                        && cx < self.span
                        && let Some(list) = self.buckets.get(&(cy * self.span + cx))
                    {
                        for &k in list {
                            let d = seg_dist2(x, y, e[k], e[k + 1], e[k + 2], e[k + 3]);
                            if d < best {
                                best = d;
                            }
                        }
                    }
                    cx += stride;
                }
            }
            // Every bucket outside ring r is at least r cells away (best is squared).
            if best <= (r as f64 * self.cell).powi(2) {
                break;
            }
        }
        best.sqrt()
    }
}

fn sample_grid(polys: &[Vec<Point>], b: &Bounds, step: f64, height: &dyn Fn(f64, f64, f64) -> f64) -> Grid {
    let pad = 1.0;
    let x0 = b.min_x - pad;
    let y0 = b.min_y - pad;
    let nx = ((b.max_x - b.min_x + 2.0 * pad) / step).ceil() as usize + 1;
    let ny = ((b.max_y - b.min_y + 2.0 * pad) / step).ceil() as usize + 1;
    let mut sd = vec![0.0; nx * ny];
    let mut h = vec![0.0; nx * ny];
    let index = EdgeIndex::new(polys, b);
    let xs: Vec<f64> = (0..nx).map(|i| x0 + i as f64 * step).collect();
    let mut inside = vec![0u8; nx];
    for j in 0..ny {
        let y = y0 + j as f64 * step;
        inside_row(polys, y, &xs, &mut inside);
        for i in 0..nx {
            let d = index.distance(xs[i], y);
            sd[j * nx + i] = if inside[i] != 0 { d } else { -d };
            h[j * nx + i] = if inside[i] != 0 { height(xs[i], y, d) } else { 0.0 };
        }
    }
    Grid { x0, y0, step, nx, ny, sd, h }
}

fn loop_box(loops: &[Vec<Point>]) -> SliceBox {
    let b = bounds(loops);
    SliceBox { x: b.min_x, y: b.min_y, w: b.max_x - b.min_x, h: b.max_y - b.min_y }
}

/// A rectangle of grid samples, inclusive.
#[derive(Clone, Copy)]
struct CellBox {
    i0: usize,
    i1: usize,
    j0: usize,
    j1: usize,
}

/// A thin region's box, and the highest surface over the thin samples themselves.
struct ThinBox {
    cells: CellBox,
    top: f64,
}

/// Cross-section at `level` clipped to the outline (and to `bx`, if given), as loops. The level is
/// a field that is positive inside it, zero on its edge and roughly a distance in viewBox units:
/// smooth across the rim, so contours interpolated between grid samples follow the real edge
/// instead of the grid.
fn level_loops(g: &Grid, level: f64, part: &VolumePart, bx: Option<CellBox>) -> Vec<Vec<Point>> {
    // A box is contoured on its own samples plus one ring outside it, where the clip is negative.
    let i0 = bx.map_or(0, |b| b.i0.saturating_sub(1));
    let i1 = bx.map_or(g.nx - 1, |b| (g.nx - 1).min(b.i1 + 1));
    let j0 = bx.map_or(0, |b| b.j0.saturating_sub(1));
    let j1 = bx.map_or(g.ny - 1, |b| (g.ny - 1).min(b.j1 + 1));
    let nx = i1 - i0 + 1;
    let ny = j1 - j0 + 1;
    let mut values = vec![0.0; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let k = (j + j0) * g.nx + i + i0;
            let sd = g.sd[k];
            // Both terms are continuous across the outline, so the min is too.
            let mut v =
                part.level_field(g.x0 + (i + i0) as f64 * g.step, g.y0 + (j + j0) as f64 * g.step, sd, level).min(sd);
            if let Some(b) = bx {
                let (ii, jj) = ((i + i0) as f64, (j + j0) as f64);
                let m = (ii - b.i0 as f64).min(b.i1 as f64 - ii).min(jj - b.j0 as f64).min(b.j1 as f64 - jj);
                v = v.min(g.step * m);
            }
            values[j * nx + i] = v;
        }
    }
    contour_loops(
        &Field { values, nx, ny, x0: g.x0 + i0 as f64 * g.step, y0: g.y0 + j0 as f64 * g.step, step: g.step },
        0.0,
    )
    .iter()
    .map(|l| simplify_loop(l, 0.04))
    .filter(|l| l.len() >= 3)
    .collect()
}

/// Boxes around the thin parts of a part's outline, grown by THIN_MARGIN and
/// merged where they overlap, in grid samples. Thin means the widest inscribed
/// circle within THIN_REACH has a radius under THIN (a speech bubble's tail, a
/// leaf, a handle ring).
fn thin_boxes(g: &Grid) -> Vec<ThinBox> {
    let (nx, ny) = (g.nx, g.ny);
    let reach = (THIN_REACH / g.step).ceil() as usize;
    // Largest inward distance within `reach` samples (a square window, rows then columns).
    let mut rows = vec![0.0f64; nx * ny];
    let mut widest = vec![0.0f64; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let mut m = 0.0f64;
            for k in i.saturating_sub(reach)..=(nx - 1).min(i + reach) {
                m = m.max(g.sd[j * nx + k]);
            }
            rows[j * nx + i] = m;
        }
    }
    for i in 0..nx {
        for j in 0..ny {
            let mut m = 0.0f64;
            for k in j.saturating_sub(reach)..=(ny - 1).min(j + reach) {
                m = m.max(rows[k * nx + i]);
            }
            widest[j * nx + i] = m;
        }
    }
    let thin = |k: usize| g.sd[k] > 0.0 && widest[k] < THIN;
    let mut seen = vec![false; nx * ny];
    let grow = (THIN_MARGIN / g.step).ceil() as usize;
    let mut boxes: Vec<ThinBox> = Vec::new();
    for start in 0..nx * ny {
        if seen[start] || !thin(start) {
            continue;
        }
        let (mut i0, mut i1, mut j0, mut j1, mut top) = (nx, 0usize, ny, 0usize, 0.0f64);
        let mut any = false;
        let mut cells = 0usize;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(k) = stack.pop() {
            let i = k % nx;
            let j = (k - i) / nx;
            cells += 1;
            any = true;
            top = top.max(g.h[k]);
            i0 = i0.min(i);
            i1 = i1.max(i);
            j0 = j0.min(j);
            j1 = j1.max(j);
            let ns = [
                if i > 0 { Some(k - 1) } else { None },
                if i < nx - 1 { Some(k + 1) } else { None },
                if j > 0 { Some(k - nx) } else { None },
                if j < ny - 1 { Some(k + nx) } else { None },
            ];
            for n in ns.into_iter().flatten() {
                if !seen[n] && thin(n) {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        // Specks are not worth a slice.
        if !any || (cells as f64) * g.step * g.step < 2.0 {
            continue;
        }
        boxes.push(ThinBox {
            cells: CellBox {
                i0: i0.saturating_sub(grow),
                i1: (nx - 1).min(i1 + grow),
                j0: j0.saturating_sub(grow),
                j1: (ny - 1).min(j1 + grow),
            },
            top,
        });
    }
    // Overlapping boxes become one, so no cross-section is drawn twice.
    let mut a = 0;
    while a < boxes.len() {
        let mut b = a + 1;
        while b < boxes.len() {
            let (p, q) = (&boxes[a], &boxes[b]);
            let (pc, qc) = (p.cells, q.cells);
            if pc.i0 > qc.i1 || qc.i0 > pc.i1 || pc.j0 > qc.j1 || qc.j0 > pc.j1 {
                b += 1;
                continue;
            }
            let merged = ThinBox {
                cells: CellBox {
                    i0: pc.i0.min(qc.i0),
                    i1: pc.i1.max(qc.i1),
                    j0: pc.j0.min(qc.j0),
                    j1: pc.j1.max(qc.j1),
                },
                top: p.top.max(q.top),
            };
            boxes[a] = merged;
            boxes.remove(b);
            b = a + 1;
        }
        a += 1;
    }
    boxes
}

/// Side profile for the slab of grid columns c0..c1, drawn as the plane at its
/// column `at`: loops in (u = -z, y), symmetric in u. Each row takes the largest
/// height across the slab, so a stack of these seen side-on adds up to the
/// solid's side silhouette instead of one tooth per column where the outline
/// slopes. Where a row run meets the outline the profile ends at the surface
/// height right there (`edge_height`: the wall of an inflated part, the rim of a
/// revolved one), not at the last sample's.
fn column_loops(g: &Grid, c0: usize, c1: usize, edge_height: &dyn Fn(f64, f64) -> f64) -> Vec<Vec<Point>> {
    let mut loops: Vec<Vec<Point>> = Vec::new();
    let mut sd = vec![f64::NEG_INFINITY; g.ny];
    let mut h = vec![0.0f64; g.ny];
    for j in 0..g.ny {
        for c in c0..=c1 {
            let k = j * g.nx + c;
            if g.sd[k] > sd[j] {
                sd[j] = g.sd[k];
            }
            if g.sd[k] > 0.0 && g.h[k] > h[j] {
                h[j] = g.h[k];
            }
        }
    }
    let y_at = |jj: usize| g.y0 + jj as f64 * g.step;
    // Where the outline crosses between an inside and an outside sample, and the surface height there.
    let edge = |inside: usize, outside: usize| {
        y_at(inside) + ((y_at(outside) - y_at(inside)) * sd[inside]) / (sd[inside] - sd[outside])
    };
    let rim = |y: f64, j: usize| {
        let mut top = 0.0f64;
        for c in c0..=c1 {
            top = top.max(edge_height(g.x0 + c as f64 * g.step, y));
        }
        h[j].min(top)
    };
    let mut j = 0;
    while j < g.ny {
        if sd[j] <= 0.0 {
            j += 1;
            continue;
        }
        let j0 = j;
        while j < g.ny && sd[j] > 0.0 {
            j += 1;
        }
        let j1 = j - 1;
        let y_top = if j0 > 0 { edge(j0, j0 - 1) } else { y_at(j0) };
        let y_bot = if j1 < g.ny - 1 { edge(j1, j1 + 1) } else { y_at(j1) };
        let mut right: Vec<Point> = vec![[if j0 > 0 { rim(y_top, j0) } else { h[j0] }, y_top]];
        for (k, &hk) in h.iter().enumerate().take(j1 + 1).skip(j0) {
            right.push([hk, y_at(k)]);
        }
        right.push([if j1 < g.ny - 1 { rim(y_bot, j1) } else { h[j1] }, y_bot]);
        if right.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max) < 0.05 {
            continue;
        }
        let left: Vec<Point> = right.iter().rev().map(|[u, y]| [-u, *y]).collect();
        let mut all = right;
        all.extend(left);
        loops.push(simplify_loop(&all, 0.04));
    }
    loops
}

/// Depth levels in (flat, flat + round): even in angle over the rounded part of the profile.
fn levels(flat: f64, round: f64, steps: usize) -> Vec<f64> {
    (1..steps).map(|k| flat + round * ((k as f64 / steps as f64) * (std::f64::consts::PI / 2.0)).sin()).collect()
}

/// Build the implied volume. `volume` scales every depth (0 = flat). Pass the body analysis of `def`.
pub fn build_volume(def: &AvatarDefinition, volume: f64, body: &BodyAnalysis) -> Volume {
    let outlines: Vec<_> = body.subpaths.iter().filter(|s| !s.hole).collect();
    let holes: Vec<_> = body.subpaths.iter().filter(|s| s.hole).collect();
    let v = if volume.is_finite() { volume.max(0.0) } else { 1.0 };
    let x_axis = revolve_axis(body);

    let parts: Vec<VolumePart> = outlines
        .iter()
        .map(|o| {
            let cuts: Vec<_> =
                holes.iter().filter(|h| h.probe.is_some_and(|p| is_inside(&o.polys, p[0], p[1]))).collect();
            let mut polys = o.polys.clone();
            for h in &cuts {
                polys.extend(h.polys.iter().cloned());
            }
            let b = bounds(&o.polys);
            let center: Point = [(b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0];
            let min_dim = (b.max_x - b.min_x).min(b.max_y - b.min_y);
            let step = 0.25f64.max(0.45f64.min(min_dim / 24.0));

            // Classify: shared horizontal axis, own vertical axis (multi-part bodies only), or inflated.
            let mut kind = PartKind::Inflate;
            let mut profile: Option<Profile> = None;
            if let Some(xa) = x_axis {
                profile = revolve_profile(&polys, false, b.min_x, b.max_x, xa);
                if profile.is_some() {
                    kind = PartKind::RevolveX;
                }
            } else if outlines.len() > 1 && cuts.is_empty() {
                profile = revolve_profile(&polys, true, b.min_y, b.max_y, center[0]);
                if profile.is_some() {
                    kind = PartKind::RevolveY;
                }
            }

            let (thickness, depth, mut rxy, mut rz, model);
            rxy = 0.0;
            rz = 0.0;
            match (kind, profile) {
                (PartKind::RevolveX, Some(p)) | (PartKind::RevolveY, Some(p)) => {
                    let radius = p.r.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    thickness = 2.0 * radius;
                    depth = 2.0 * radius * v;
                    model = if kind == PartKind::RevolveX {
                        HeightModel::RevolveX { profile: p, axis: x_axis.unwrap_or(0.0), v }
                    } else {
                        HeightModel::RevolveY { profile: p, axis: center[0], v }
                    };
                }
                _ => {
                    let coarse = sample_grid(&polys, &b, step.max(0.6), &|_, _, _| 0.0);
                    thickness = 2.0 * coarse.sd.iter().fold(0.0f64, |m, d| m.max(*d));
                    depth = part_depth(thickness, &b, v);
                    rxy = (thickness / 2.0).min(depth / 2.0) * 0.9;
                    rz = (depth / 2.0) * 0.85;
                    model = HeightModel::Inflate;
                }
            }
            let kind = if matches!(model, HeightModel::Inflate) { PartKind::Inflate } else { kind };

            let core_d = format!("{}{}", o.d, cuts.iter().map(|h| h.d.as_str()).collect::<String>());
            let mut part = VolumePart {
                kind,
                d: core_d.clone(),
                polys: polys.clone(),
                bounds: b,
                center,
                thickness,
                depth,
                rxy,
                rz,
                axis: match kind {
                    PartKind::RevolveX => x_axis,
                    PartKind::RevolveY => Some(center[0]),
                    PartKind::Inflate => None,
                },
                z_slices: Vec::new(),
                x_slices: Vec::new(),
                model,
            };

            let core = VolumeSlice {
                axis: SliceAxis::Z,
                at: 0.0,
                d: core_d,
                loops: polys.clone(),
                even_odd: false,
                bbox: SliceBox { x: b.min_x, y: b.min_y, w: b.max_x - b.min_x, h: b.max_y - b.min_y },
                inset: 0.0,
            };
            let mut z_slices = vec![core];
            let mut x_slices: Vec<VolumeSlice> = Vec::new();
            let half = depth / 2.0;
            if half > 0.05 {
                let g = sample_grid(&polys, &b, step, &|x, y, sd| part.height_of(x, y, sd));
                let steps = if kind == PartKind::Inflate {
                    4usize.max(14usize.min(js_round(rz / 0.8) as usize))
                } else {
                    5usize.max(18usize.min(js_round(half / 1.1) as usize))
                };
                // Inflated parts keep a straight wall up to half - rz; one slice at its end carries it.
                let ls: Vec<f64> = if kind == PartKind::Inflate {
                    let mut l = vec![half - rz];
                    l.extend(levels(half - rz, rz, steps));
                    l
                } else {
                    levels(0.0, half, steps)
                };
                let add_level = |level: f64, bx: Option<CellBox>, z_slices: &mut Vec<VolumeSlice>| -> bool {
                    let loops = level_loops(&g, level, &part, bx);
                    if loops.is_empty() {
                        return false;
                    }
                    let inset = loops
                        .iter()
                        .flatten()
                        .enumerate()
                        .filter(|(k, _)| k % 3 == 0)
                        .map(|(_, p)| edge_distance(&polys, p[0], p[1]))
                        .fold(f64::INFINITY, f64::min);
                    let d = loops_to_path(&loops);
                    let bbox = loop_box(&loops);
                    for at in [level, -level] {
                        z_slices.push(VolumeSlice {
                            axis: SliceAxis::Z,
                            at,
                            d: d.clone(),
                            loops: loops.clone(),
                            even_odd: true,
                            bbox,
                            inset,
                        });
                    }
                    true
                };
                let mut used = vec![0.0f64];
                for level in ls {
                    if level >= 0.02 && add_level(level, None, &mut z_slices) {
                        used.push(level);
                    }
                }
                // Thin parts of an inflated outline (deep, narrow fins): more cross-sections between the levels,
                // the straight wall included, each clipped to its box. Revolved parts' slices span their thin side.
                let thin = if kind == PartKind::Inflate { thin_boxes(&g) } else { Vec::new() };
                for tb in &thin {
                    let mut k = 1;
                    while k < used.len() && used[k - 1] < tb.top {
                        let a = used[k - 1];
                        let gap = used[k] - a;
                        let m = (gap / FINE_Z - 1e-9).ceil() as usize;
                        let mut q = 1;
                        while q < m && (a + (gap * q as f64) / m as f64) < tb.top {
                            add_level(a + (gap * q as f64) / m as f64, Some(tb.cells), &mut z_slices);
                            q += 1;
                        }
                        k += 1;
                    }
                }
                z_slices.sort_by(|m, n| m.at.total_cmp(&n.at));

                let width = b.max_x - b.min_x;
                let every = ((1.1f64.max(width / MAX_X_SLICES) / g.step).round()).max(1.0) as usize;
                let edge_height = |x: f64, y: f64| part.height_of(x, y, 0.0);
                // One plane per `every` grid columns, standing for the whole slab around it.
                let before = (every - 1) / 2;
                let mut i = 1;
                while i < g.nx - 1 {
                    let loops = column_loops(
                        &g,
                        i.saturating_sub(before),
                        (g.nx - 1).min(i - before.min(i) + every - 1),
                        &edge_height,
                    );
                    if !loops.is_empty() {
                        let d = loops_to_path(&loops);
                        let bbox = loop_box(&loops);
                        x_slices.push(VolumeSlice {
                            axis: SliceAxis::X,
                            at: g.x0 + i as f64 * g.step,
                            d,
                            loops,
                            even_odd: true,
                            bbox,
                            inset: 0.0,
                        });
                    }
                    i += every;
                }
            }
            part.z_slices = z_slices;
            part.x_slices = x_slices;
            part
        })
        .collect();

    let eye_scale = boosted_eye_scale(body, &def.eyes);
    // The eyes ride on whichever part is highest under their centers.
    let centers = eye_centers(&def.eyes);
    let mut eye_part = 0;
    let mut best_z = f64::NEG_INFINITY;
    for (i, part) in parts.iter().enumerate() {
        let zs: Vec<f64> = centers.iter().map(|[x, y]| part.height(*x, *y)).collect();
        if zs.iter().any(|z| *z <= 0.0) {
            continue;
        }
        let z = zs.iter().copied().fold(f64::INFINITY, f64::min);
        if z > best_z {
            best_z = z;
            eye_part = i;
        }
    }
    let host = parts.get(eye_part);
    // Flat eye planes sit at the highest surface under the eye and its gaze travel, so the body never cuts them.
    let w = (EYE_W * eye_scale) / 2.0 + GAZE_X * eye_scale;
    let hh = (EYE_H * eye_scale) / 2.0 + GAZE_Y * eye_scale;
    let eye_z_of = |[x, y]: Point| {
        let mut top = 0.0f64;
        for fx in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for fy in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                top = top.max(host.map_or(0.0, |h| h.height(x + fx * w, y + fy * hh)));
            }
        }
        top
    };
    let eye_z = [eye_z_of(centers[0]), eye_z_of(centers[1])];

    let normal = |x: f64, y: f64| -> Vec3 {
        let Some(host) = host else { return [0.0, 0.0, 1.0] };
        let e = 0.75;
        let gx = (host.height(x + e, y) - host.height(x - e, y)) / (2.0 * e);
        let gy = (host.height(x, y + e) - host.height(x, y - e)) / (2.0 * e);
        let n = gx.hypot(gy).hypot(1.0);
        [-gx / n, -gy / n, 1.0 / n]
    };
    let half = (EYE_W * eye_scale) / 2.0;
    let eye_normals = [
        vec![
            normal(centers[0][0] - half, centers[0][1]),
            normal(centers[0][0], centers[0][1]),
            normal(centers[0][0] + half, centers[0][1]),
        ],
        vec![
            normal(centers[1][0] - half, centers[1][1]),
            normal(centers[1][0], centers[1][1]),
            normal(centers[1][0] + half, centers[1][1]),
        ],
    ];

    Volume { parts, bounds: body.bounds, eye_part, eye_z, eye_scale, eye_normals }
}

/// Convenience: analyse the definition's body and build its volume.
pub fn build_volume_of(def: &AvatarDefinition, volume: f64) -> Result<Volume, super::path::PathError> {
    let body = analyze_body(&def.body_path)?;
    Ok(build_volume(def, volume, &body))
}

/// Largest eye scale up to EYE_BOOST × the definition's whose drawn eyes keep EYE_BOOST_PAD from the outline.
pub fn boosted_eye_scale(body: &BodyAnalysis, eyes: &AvatarEyes) -> f64 {
    for k in [EYE_BOOST, 1.38, 1.3, 1.22, 1.15, 1.08] {
        let scaled = AvatarEyes { scale: eyes.scale * k, ..*eyes };
        if drawn_eyes_fit(&body.polys, &scaled, 0.0, 0.0, EYE_BOOST_PAD).is_none() {
            return eyes.scale * k;
        }
    }
    eyes.scale
}

#[derive(Debug, Clone, Copy)]
pub struct FaceRoom {
    /// Drawn eye size relative to the definition's (1..EYE_BOOST).
    pub boost: f64,
    /// Gap between the drawn eyes at rest (pill and happy arc) and the outline, viewBox units.
    pub clearance: f64,
    /// Gaze travel left at the drawn size (see `gaze_range`).
    pub gaze: GazeRange,
    /// True when the face meets the design target: full boost, FACE_ROOM_TARGET clearance, full gaze.
    pub ok: bool,
}

/// How much room the face has as the renderer will actually draw it.
pub fn face_room(def: &AvatarDefinition, body: &BodyAnalysis) -> FaceRoom {
    let scale = boosted_eye_scale(body, &def.eyes);
    let eyes = AvatarEyes { scale, ..def.eyes };
    let clearance = eye_clearance(&body.polys, &eyes);
    let gaze = gaze_range(&body.polys, &eyes);
    let boost = js_round((scale / def.eyes.scale) * 100.0) / 100.0;
    let ok = boost >= EYE_BOOST - 1e-9
        && clearance >= FACE_ROOM_TARGET
        && gaze.left.min(gaze.right).min(gaze.up).min(gaze.down) == 1.0;
    FaceRoom { boost, clearance, gaze, ok }
}

#[allow(dead_code)]
const _: fn(&[Vec<Point>], f64, f64) -> f64 = inside_distance;
#[allow(dead_code)]
const _ERO: fn(f64, f64, f64, f64) -> f64 = erosion_at;
