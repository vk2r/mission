//! Iso-contours of a sampled scalar field (marching squares), assembled into
//! closed loops and simplified (ported from `contour.ts`). Used once per definition to turn a
//! part's depth field into plain cross-section paths, so slices need no SVG masks.

use std::collections::{HashMap, HashSet};

use super::color::js_round;
use super::path::Point;

pub struct Field {
    /// Row-major samples, nx * ny. The outermost ring should be below every level.
    pub values: Vec<f64>,
    pub nx: usize,
    pub ny: usize,
    pub x0: f64,
    pub y0: f64,
    pub step: f64,
}

struct Links {
    points: HashMap<usize, Point>,
    links: HashMap<usize, Vec<usize>>,
    /// Keys in first-linked order, which is the order JS `Map.keys()` walks them.
    order: Vec<usize>,
}

/// Closed loops where the field crosses `level` (inside = value ≥ level).
pub fn contour_loops(field: &Field, level: f64) -> Vec<Vec<Point>> {
    let Field { values, nx, ny, x0, y0, step } = field;
    let (nx, ny, x0, y0, step) = (*nx, *ny, *x0, *y0, *step);
    let at = |i: usize, j: usize| values[j * nx + i] - level;
    // Edge keys: horizontal edge (i,j)-(i+1,j) = 2*(j*nx+i), vertical (i,j)-(i,j+1) = 2*(j*nx+i)+1.
    let mut st = Links { points: HashMap::new(), links: HashMap::new(), order: Vec::new() };
    let point = |st: &mut Links, key: usize| {
        if st.points.contains_key(&key) {
            return;
        }
        let idx = key >> 1;
        let i = idx % nx;
        let j = (idx - i) / nx;
        let a = at(i, j);
        let p = if key & 1 == 1 {
            let b = at(i, j + 1);
            let t = a / (a - b);
            [x0 + i as f64 * step, y0 + (j as f64 + t) * step]
        } else {
            let b = at(i + 1, j);
            let t = a / (a - b);
            [x0 + (i as f64 + t) * step, y0 + j as f64 * step]
        };
        st.points.insert(key, p);
    };
    let push = |st: &mut Links, from: usize, to: usize| {
        if !st.links.contains_key(&from) {
            st.order.push(from);
        }
        st.links.entry(from).or_default().push(to);
    };
    let link = |st: &mut Links, p: usize, q: usize| {
        point(st, p);
        point(st, q);
        push(st, p, q);
        push(st, q, p);
    };

    for j in 0..ny.saturating_sub(1) {
        for i in 0..nx.saturating_sub(1) {
            let tl = at(i, j) >= 0.0;
            let tr = at(i + 1, j) >= 0.0;
            let br = at(i + 1, j + 1) >= 0.0;
            let bl = at(i, j + 1) >= 0.0;
            let code = (if tl { 8 } else { 0 })
                | (if tr { 4 } else { 0 })
                | (if br { 2 } else { 0 })
                | (if bl { 1 } else { 0 });
            if code == 0 || code == 15 {
                continue;
            }
            let top = 2 * (j * nx + i);
            let bottom = 2 * ((j + 1) * nx + i);
            let left = 2 * (j * nx + i) + 1;
            let right = 2 * (j * nx + i + 1) + 1;
            if code == 10 || code == 5 {
                let center = (at(i, j) + at(i + 1, j) + at(i + 1, j + 1) + at(i, j + 1)) / 4.0 >= 0.0;
                // Diagonal corners: the center decides whether they connect through the cell.
                if (code == 10) == center {
                    link(&mut st, top, right);
                    link(&mut st, bottom, left);
                } else {
                    link(&mut st, left, top);
                    link(&mut st, right, bottom);
                }
                continue;
            }
            let mut crossed = Vec::with_capacity(2);
            if tl != tr {
                crossed.push(top);
            }
            if tr != br {
                crossed.push(right);
            }
            if br != bl {
                crossed.push(bottom);
            }
            if bl != tl {
                crossed.push(left);
            }
            link(&mut st, crossed[0], crossed[1]);
        }
    }

    let mut loops: Vec<Vec<Point>> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    for &start in &st.order {
        if seen.contains(&start) {
            continue;
        }
        let mut lp: Vec<Point> = Vec::new();
        let mut prev: Option<usize> = None;
        let mut cur = start;
        while !seen.contains(&cur) {
            seen.insert(cur);
            lp.push(st.points[&cur]);
            let next = st.links[&cur].iter().copied().find(|n| Some(*n) != prev && !seen.contains(n));
            let Some(next) = next else { break };
            prev = Some(cur);
            cur = next;
        }
        if lp.len() >= 3 {
            loops.push(lp);
        }
    }
    loops
}

fn perpendicular(p: Point, a: Point, b: Point) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = dx.hypot(dy);
    if len == 0.0 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    (dx * (a[1] - p[1]) - dy * (a[0] - p[0])).abs() / len
}

/// Ramer-Douglas-Peucker on a closed loop.
pub fn simplify_loop(lp: &[Point], tolerance: f64) -> Vec<Point> {
    if lp.len() <= 8 {
        return lp.to_vec();
    }
    // Split at the point farthest from the first so the closed loop becomes two open chains.
    let mut far = 0;
    let mut best = -1.0;
    for (k, p) in lp.iter().enumerate().skip(1) {
        let d = (p[0] - lp[0][0]).hypot(p[1] - lp[0][1]);
        if d > best {
            best = d;
            far = k;
        }
    }
    let mut keep = vec![false; lp.len()];
    keep[0] = true;
    keep[far] = true;
    let mut stack: Vec<(usize, usize)> = vec![(0, far), (far, lp.len())];
    while let Some((s, e)) = stack.pop() {
        let end = lp[e % lp.len()];
        let mut idx: Option<usize> = None;
        let mut max = tolerance;
        for k in s + 1..e {
            let d = perpendicular(lp[k], lp[s], end);
            if d > max {
                max = d;
                idx = Some(k);
            }
        }
        if let Some(idx) = idx {
            keep[idx] = true;
            stack.push((s, idx));
            stack.push((idx, e));
        }
    }
    lp.iter().zip(&keep).filter(|(_, k)| **k).map(|(p, _)| *p).collect()
}

/// Path numbers are hundredths; vertices snap to even ones so edge midpoints land exactly on the grid.
const UNIT: f64 = 100.0;

/// A grid value as a path number, as short as SVG allows: no leading zero, no `-0`.
fn num(k: f64) -> String {
    let a = k.abs() as i64;
    let whole = a / 100;
    let frac = a % 100;
    let f = if frac == 0 {
        String::new()
    } else if frac % 10 == 0 {
        format!(".{}", frac / 10)
    } else if frac < 10 {
        format!(".0{frac}")
    } else {
        format!(".{frac}")
    };
    let head = if whole != 0 || f.is_empty() { whole.to_string() } else { String::new() };
    format!("{}{head}{f}", if k < 0.0 { "-" } else { "" })
}

/// Turns sharper than this (radians) stay corners; gentler ones are drawn as curves.
const CORNER: f64 = 0.6;

/// A gentle turn between long edges is a corner of the outline too, not a facet of a traced curve:
/// it stays sharp once its curve would leave the traced loop by more than this (viewBox units).
const SAG: f64 = 0.1;

struct Emitter {
    d: String,
    cmd: char,
    last: String,
}

impl Emitter {
    fn emit(&mut self, c: char, ks: &[f64]) {
        if c != self.cmd {
            self.d.push(c);
            self.cmd = c;
            self.last.clear();
        }
        for k in ks {
            let t = num(*k);
            // A separator is only needed where the next number could be read as part of the last one.
            if !self.last.is_empty() && !t.starts_with('-') && !(t.starts_with('.') && self.last.contains('.')) {
                self.d.push(' ');
            }
            self.d.push_str(&t);
            self.last = t;
        }
    }
}

/// Compact closed path from loops (draw with fill-rule evenodd). Traced loops are polylines, so
/// gentle turns are drawn as quadratics between edge midpoints, which hides their facets at large
/// sizes; real corners stay sharp. Relative commands, smooth t segments and implicit repeats keep
/// the string short.
pub fn loops_to_path(loops: &[Vec<Point>]) -> String {
    let mut e = Emitter { d: String::new(), cmd: '\0', last: String::new() };
    for lp in loops {
        let n = lp.len();
        let v: Vec<[f64; 2]> =
            lp.iter().map(|[x, y]| [2.0 * js_round(x * UNIT / 2.0), 2.0 * js_round(y * UNIT / 2.0)]).collect();
        let corner: Vec<bool> = lp
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let a = lp[(k + n - 1) % n];
                let b = lp[(k + 1) % n];
                let cross = (p[0] - a[0]) * (b[1] - p[1]) - (p[1] - a[1]) * (b[0] - p[0]);
                // The curve passes a quarter of p's height over the chord a-b away from p.
                cross.atan2((p[0] - a[0]) * (b[0] - p[0]) + (p[1] - a[1]) * (b[1] - p[1])).abs() > CORNER
                    || cross.abs() > 4.0 * SAG * (b[0] - a[0]).hypot(b[1] - a[1])
            })
            .collect();
        let mid = |k: usize| [(v[k % n][0] + v[(k + 1) % n][0]) / 2.0, (v[k % n][1] + v[(k + 1) % n][1]) / 2.0];
        let [mut cx, mut cy] = mid(0);
        e.emit('M', &[cx, cy]);
        let mut smooth = false;
        for k in 1..=n {
            let [px, py] = v[k % n];
            if corner[k % n] {
                // A midpoint between two corners lies on their straight edge, so it is only kept to start a curve.
                e.emit('l', &[px - cx, py - cy]);
                cx = px;
                cy = py;
                if k < n && !corner[(k + 1) % n] {
                    let [mx, my] = mid(k);
                    e.emit('l', &[mx - cx, my - cy]);
                    cx = mx;
                    cy = my;
                }
                smooth = false;
            } else {
                let [mx, my] = mid(k);
                // After a curve, the reflected control point is exactly this vertex (it mirrors the last one
                // across their shared midpoint), so t needs only the end point.
                if smooth {
                    e.emit('t', &[mx - cx, my - cy]);
                } else {
                    e.emit('q', &[px - cx, py - cy, mx - cx, my - cy]);
                }
                cx = mx;
                cy = my;
                smooth = true;
            }
        }
        e.d.push('Z');
        e.cmd = 'Z';
        e.last.clear();
    }
    e.d
}
