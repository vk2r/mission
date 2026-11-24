//! What a state draws besides the body: the ribbons that circle a working bot, and the marks a
//! thinking or blocked bot turns into (three dots, an exclamation mark). Pure geometry in viewBox
//! units, from the body's bounds, a seed and the animation clock (ported from `effects.ts`).

use std::f64::consts::PI;

use super::color::js_round;
use super::motion::noise;
use super::path::{Bounds, Point, fmt};

const TAU: f64 = PI * 2.0;

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    hi.min(lo.max(v))
}

fn smooth(u: f64) -> f64 {
    let x = clamp(u, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

// ---------------------------------------------------------------------------
// Ribbons

/// One piece of a ribbon: the part of it in front of the body, or the part behind.
#[derive(Debug, Clone)]
pub struct RibbonPiece {
    pub d: String,
    pub front: bool,
    pub bbox: Rect,
    /// Which ribbon the piece belongs to. In a celebration, 0..8: hoop `ribbon / LANES`, lane
    /// `ribbon % LANES` (from the inner ring out). A working turn has one hoop with three to five
    /// ribbons, numbered 0..4 from the inner ring out.
    pub ribbon: usize,
    /// The stretch of its ribbon the piece covers, 0 (tail) to 1 (head), and the centre of the
    /// ribbon at either end of that stretch, for a gradient along it. A ribbon comes in a few
    /// stretches, each a little past the next where they meet so no seam shows.
    pub along: [f64; 2],
    pub from: [f64; 2],
    pub to: [f64; 2],
}

// The ribbons ride hoops round the body's centre, laid out as PR #14 measured them from the
// Grok Bot demo, at one size for every avatar. Lengths are in design units: the demo's
// 228.541-unit space, which its bot fills; here that space is a body of the typical size,
// BODY units across, so one design unit is BODY / DESIGN avatar units whatever the avatar.
//
// A hoop is a ring of radius RING tipped HOOP_TILT out of the screen, so it shows as a flat ellipse, and
// rolled in the picture. Its near side is the top: a ribbon crosses in front of the body along
// the top and behind it along the bottom. A working bot uses one hoop, rolled HOOP_ROLL; a
// celebration three, each a third of a half turn further round, so they cross into a cage.
//
// Each hoop carries LANES ribbons on rings LADDER units apart in all, outward from its ring.
// A ribbon starts at the top of its hoop and goes round with the body's turn (`sweep`), each at
// its own share of it (FOLLOW) plus a drift of its own (DRIFT), so the ribbons on one hoop part
// as they go; its ring widens as it goes (WIDEN), and its tail trails up to ARC behind its head.
const DESIGN: f64 = 228.541;
/// The typical body across its larger side (the median of the starters and gallery).
const BODY: f64 = 80.0;
const UNIT: f64 = BODY / DESIGN;
/// A hoop's inner ring stands this far outside the body along the hoop's own direction: the hoop's
/// sides, where its ribbons pass behind the body, lie just off the outline (the inner ribbon's
/// edge included), whatever the shape. RING is the typical body's (half its widest row, 38.8,
/// the median of the starters and gallery, × 116/114.27), for callers that give no outline.
const RIM_MARGIN: f64 = 2.0;
const RING: f64 = 38.8 * (116.0 / 114.2705);

/// How far the outlines reach from (cx, cy): `at` along a direction (radians), either way, worked
/// out exactly each time it is asked (a hoop asks once a frame, along its own roll); `max` the
/// farthest over all directions, from RIM_STEPS samples over a half turn.
const RIM_STEPS: usize = 180;

/// How far `outlines` reach from (cx, cy) along a direction (radians), either way, exactly.
fn reach_along(outlines: &[Vec<Vec<Point>>], cx: f64, cy: f64, a: f64) -> f64 {
    let (ux, uy) = (a.cos(), a.sin());
    let mut far = 0.0f64;
    for poly in outlines.iter().flatten() {
        for i in 0..poly.len() {
            let [x1, y1] = poly[i];
            let [x2, y2] = poly[(i + 1) % poly.len()];
            let (ex, ey) = (x2 - x1, y2 - y1);
            let den = ux * ey - uy * ex;
            if den.abs() < 1e-12 {
                continue;
            }
            // Where the line through the centre crosses this edge: t along the line, s along the edge.
            let t = ((x1 - cx) * ey - (y1 - cy) * ex) / den;
            let s = ((x1 - cx) * uy - (y1 - cy) * ux) / den;
            if (0.0..=1.0).contains(&s) {
                far = far.max(t.abs());
            }
        }
    }
    far
}

/// How each hoop is rolled in the picture, for an avatar's seed: hoop 0 is a working turn's.
fn hoop_rolls(seed: u32) -> Vec<f64> {
    (0..HOOPS)
        .map(|i| HOOP_ROLL + (i as f64 * PI) / HOOPS as f64 + (noise(seed, 450 + i as i64) - 0.5) * HOOP_JITTER)
        .collect()
}

/// Where a body's hoops are centred, and how far the body reaches from there (see `rim_of`).
#[derive(Debug, Clone)]
pub struct Rim {
    pub cx: f64,
    pub cy: f64,
    pub max: f64,
    outlines: Vec<Vec<Vec<Point>>>,
    /// Where the reach is measured from: the centre the rim was made for, which stays put when
    /// `moved` carries the hoops' centre along with the body (as upstream's `at` closure does).
    origin: Point,
}

impl Rim {
    /// How far the body reaches from its own centre along `angle`, either way.
    pub fn at(&self, angle: f64) -> f64 {
        reach_along(&self.outlines, self.origin[0], self.origin[1], angle)
    }

    /// The same body outlines and reach, with the hoops' centre moved to (cx, cy).
    pub fn moved(&self, cx: f64, cy: f64) -> Rim {
        Rim { cx, cy, ..self.clone() }
    }
}

pub fn rim_of(outlines: Vec<Vec<Vec<Point>>>, cx: f64, cy: f64) -> Rim {
    let max = (0..RIM_STEPS)
        .map(|k| reach_along(&outlines, cx, cy, (k as f64 * PI) / RIM_STEPS as f64))
        .fold(f64::NEG_INFINITY, f64::max);
    // A spike between two samples can reach a little further than either: allow for a degree's sweep.
    Rim { cx, cy, max: max * 1.02, outlines, origin: [cx, cy] }
}

const HOOP_TILT: f64 = 0.386;
const HOOP_ROLL: f64 = 0.157;
/// A hoop's roll jitter (full range, radians).
const HOOP_JITTER: f64 = 0.24;
/// A hoop's tilt jitter. The ribbons on one hoop share its roll and tilt exactly: they overlap
/// side by side, and only on one plane do they all pass behind the body along the same line, so
/// which of them lies on top never flips where they do.
const HOOP_TILT_JITTER: f64 = 0.08;
const LADDER: f64 = 38.0;
const ARC: [f64; 2] = [2.2, 3.4];
const FOLLOW: [f64; 2] = [0.74, 0.94];
/// Radians per second a ribbon drifts on by itself, and design units per second its ring widens.
const DRIFT: [f64; 2] = [0.5, 1.1];
const WIDEN: f64 = 2.5;
/// How far behind the turn a ribbon starts, at most (radians).
const START: f64 = 0.18;
// A ribbon's width: a size per ribbon (design units; wider when there are only three, narrower
// among nine), times BRUSH. It narrows to FAR of that while its head is behind the body, is never
// wider than a third of its own length on screen, and runs from half that width at its tail to
// the full width at its head, round at both ends. It grows in over GROW_S and thins as it draws back in.
const SIZE_FEW: [f64; 2] = [8.0, 10.5];
const SIZE_FOUR: [f64; 2] = [6.6, 8.6];
const SIZE_MANY: [f64; 2] = [5.6, 7.4];
/// A working turn lets out three to five ribbons on its one hoop (per avatar); a celebration three on each.
const WORK_LANES: [f64; 2] = [3.0, 5.0];

fn size_for(count: usize) -> [f64; 2] {
    if count <= 3 {
        SIZE_FEW
    } else if count == 4 {
        SIZE_FOUR
    } else {
        SIZE_MANY
    }
}

const BRUSH: f64 = 1.7 * 1.98;
const FAR: f64 = 0.72;
const GROW_S: f64 = 0.34;
/// The ribbons on a turn are let out one after another, this long apart (seconds).
const STAGGER: [f64; 2] = [0.055, 0.105];
/// How fast the sweep goes on average (radians per second), working and celebrating: the drift
/// and widening are timed against it, so a ribbon is placed from the sweep alone.
const SWEEP_RATE: [f64; 2] = [6.3, 15.0];
/// The longest a sweep lasts, seconds: the widening is bounded by it.
const SWEEP_LONGEST: f64 = 4.0;
pub const LANES: usize = 3;
pub const HOOPS: usize = 3;
const SAMPLES: usize = 24;
/// A ribbon is cut into stretches this many samples long, each with a gradient of its own, so its colour follows its length even where it curves round.
const STRETCH: usize = 6;
/// How far either side of a layer change a ribbon is sampled, radians: the piece behind runs this far on under the piece in front.
const SLIVER: f64 = 0.02;
/// Points on each round end of a ribbon.
const CAP: usize = 7;

fn mix(range: [f64; 2], u: f64) -> f64 {
    range[0] + (range[1] - range[0]) * u
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RibbonReach {
    pub radius: f64,
    pub width: f64,
    pub band: f64,
}

/// The radius of the farthest ring a ribbon can ride round a body whose outline reaches `rim_max`
/// from its centre at most, the widest a ribbon gets, and how far a ribbon reaches out from its
/// ring (half that). Widths are the same for every avatar; the rings follow its outline.
pub fn ribbon_reach(rim_max: Option<f64>) -> RibbonReach {
    let width = SIZE_FEW[1] * BRUSH * UNIT;
    let ring = rim_max.map_or(RING, |m| m + RIM_MARGIN);
    RibbonReach { radius: ring + (LADDER + WIDEN * SWEEP_LONGEST) * UNIT, width, band: width / 2.0 }
}

/// The ribbons at one moment, as filled outlines: round at both ends, a little narrower at the
/// tail than at the head and narrower behind the body, and split where a ribbon passes from
/// behind the body to in front of it. `presence` 0..1 draws them out and back in. `cage` picks
/// the layout: a working turn's 3 to 5 ribbons on one hoop (below 0.5), or a celebration's three
/// on each of three hoops. `sweep` (radians, signed) is how far the body's turn has carried
/// them; between turns they are not drawn. Each ribbon narrows where its hoop curves tighter than
/// half its width, so its outline never folds over itself. `rim` gives the hoops' centre (the
/// middle of the body's mass) and how far the body reaches from it along a direction: a working
/// turn's hoop stands just outside the body along its own direction; a celebration's three share
/// one radius, the largest any of them needs, so the cage stays round. Either way the ribbons pass
/// behind the body only off its outline.
#[allow(clippy::too_many_arguments)]
pub fn ribbon_pieces(
    b: &Bounds,
    presence: f64,
    seed: u32,
    _time: f64,
    sweep: f64,
    cage: f64,
    rim: Option<&Rim>,
) -> Vec<RibbonPiece> {
    let k = clamp(presence, 0.0, 1.0);
    let more = if cage >= 0.5 { 1.0 } else { 0.0 };
    if k <= 0.01 {
        return Vec::new();
    }
    let (cx, cy) = match rim {
        Some(r) => (r.cx, r.cy),
        None => ((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0),
    };
    let rolls = hoop_rolls(seed);
    let cage_ring = match rim {
        Some(r) => (0..3).map(|i| r.at(rolls[i])).fold(0.0f64, f64::max) + RIM_MARGIN,
        None => RING,
    };
    let unit = UNIT;
    let dir = if sweep < 0.0 { -1.0 } else { 1.0 };
    // How long the sweep has been going, from how far it has got.
    let since = sweep.abs() / mix(SWEEP_RATE, more);
    let mut pieces: Vec<RibbonPiece> = Vec::new();
    let hoops = if more > 0.0 { HOOPS } else { 1 };
    for (i, &hoop_roll) in rolls.iter().enumerate().take(hoops) {
        let hoop_tilt = HOOP_TILT + (noise(seed, 460 + i as i64) - 0.5) * HOOP_TILT_JITTER;
        let ring = if more > 0.0 {
            cage_ring
        } else if let Some(r) = rim {
            r.at(hoop_roll) + RIM_MARGIN
        } else {
            RING
        };
        let lanes = if i != 0 || more > 0.0 { LANES } else { js_round(mix(WORK_LANES, noise(seed, 47))) as usize };
        for l in 0..lanes {
            let n = |m: i64| noise(seed, 500 + (i * LANES + l) as i64 * 11 + m);
            // Let out in turn, lane by lane across the hoops: how long ago this one was, and how far
            // the turn had got by then.
            let order = if more > 0.0 { l * HOOPS + i } else { l };
            let at = order as f64 * mix(STAGGER, n(7));
            let life = since - at;
            if life <= 0.0 {
                continue;
            }
            let before = if since > 0.0 { sweep.abs() * (at / since) } else { 0.0 };
            let roll = hoop_roll;
            let tilt = hoop_tilt;
            let (flat, depth) = (tilt.sin(), tilt.cos());
            let (cr, sr) = (roll.cos(), roll.sin());
            let r = ring + (l as f64 * LADDER * unit) / (lanes as f64 - 1.0) + mix([0.0, WIDEN], n(2)) * unit * life;
            // The orbit angle (0 = the top of the hoop) of the head: it starts where the turn was when
            // it was let out, a little behind, and goes its own share of the rest. Its tail trails it by
            // as far as it has gone, up to its arc; drawing back in shortens and thins it.
            let travel = (sweep.abs() - before) * mix(FOLLOW, n(3)) + mix(DRIFT, n(4)) * life;
            let head = dir * (before + travel - START * n(5));
            let ret = 1.0 - clamp(k * 1.25, 0.0, 1.0);
            let length = mix(ARC, n(6)).min(travel) * (1.0 - ret * ret * (3.0 - 2.0 * ret));
            if length <= 0.02 {
                continue;
            }
            let size = mix(size_for(lanes), n(8)) * (1.0 - more) + mix(SIZE_MANY, n(8)) * more;
            let near_head = clamp(head.cos() * depth, 0.0, 1.0);
            let mut full = size
                * BRUSH
                * unit
                * (FAR + (1.0 - FAR) * near_head)
                * smooth(life / GROW_S)
                * (1.0 - 0.72 * ret * ret);
            // A point of the ribbon is in front of the body on the near half of its hoop (depth above 0).
            let lam_at = |u: f64| head - dir * length * (1.0 - u);
            let point = |lam: f64| -> [f64; 2] {
                let a = lam.sin();
                let y = -lam.cos() * flat;
                [cx + r * (a * cr - y * sr), cy + r * (a * sr + y * cr)]
            };
            // Where along the ribbon (0 tail .. 1 head) to sample it: evenly, plus exactly where it
            // passes from one layer to the other (the hoop's sides, where its depth is 0) and a sliver
            // either side, so every ribbon on a hoop changes layer along the same line.
            let along = |lam: f64| 1.0 - (head - lam) / (dir * length);
            let sliver = SLIVER / length;
            let mut ss: Vec<f64> = (0..=SAMPLES).map(|j| j as f64 / SAMPLES as f64).collect();
            let lo = head.min(head - dir * length);
            let hi = head.max(head - dir * length);
            let mut side = ((lo - PI / 2.0) / PI).ceil();
            while PI / 2.0 + side * PI < hi {
                let u = along(PI / 2.0 + side * PI);
                for d in [-sliver, 0.0, sliver] {
                    if u + d > 0.0 && u + d < 1.0 {
                        ss.push(u + d);
                    }
                }
                side += 1.0;
            }
            ss.sort_by(|x, y| x.total_cmp(y));
            // A sample exactly on a side belongs with the stretch after it.
            let near: Vec<bool> = ss.iter().map(|u| (lam_at(*u) + dir * 1e-6).cos() > 0.0).collect();
            // Centre, half width, unit normal (on screen) at each sample, tail to head.
            let mut mid: Vec<[f64; 2]> = Vec::new();
            let mut halfs: Vec<f64> = Vec::new();
            let mut norm: Vec<[f64; 2]> = Vec::new();
            let mut bend: Vec<f64> = Vec::new();
            for u in &ss {
                let lam = lam_at(*u);
                // On the hoop before its roll: the ellipse's normal.
                let nx = lam.sin() * flat;
                let ny = -lam.cos();
                let nl = {
                    let h = nx.hypot(ny);
                    if h == 0.0 { 1.0 } else { h }
                };
                let (ux, uy) = (nx / nl, ny / nl);
                mid.push(point(lam));
                norm.push([ux * cr - uy * sr, ux * sr + uy * cr]);
                halfs.push(0.5 + 0.5 * u);
                // How tightly the hoop curves here, as a radius: a ribbon wider than it would fold.
                let (c, sn) = (lam.cos(), lam.sin());
                bend.push((r * (c * c + flat * flat * sn * sn).powf(1.5)) / flat);
            }
            let last = ss.len() - 1;
            // Never wider than a third of its own length on screen.
            let mut on_screen = 0.0;
            for j in 1..=last {
                on_screen += (mid[j][0] - mid[j - 1][0]).hypot(mid[j][1] - mid[j - 1][1]);
            }
            full = full.min(0.34 * on_screen);
            for j in 0..=last {
                halfs[j] = (halfs[j] * (full / 2.0)).min(0.9 * bend[j]);
            }
            let edge = |j: usize, sign: f64| -> [f64; 2] {
                [mid[j][0] + sign * norm[j][0] * halfs[j], mid[j][1] + sign * norm[j][1] * halfs[j]]
            };
            // A half circle round an end, from one edge to the other through the point ahead of it (or behind).
            let cap = |j: usize, ahead: bool| -> Vec<[f64; 2]> {
                let o = if ahead { mid[j - 1] } else { mid[j + 1] };
                let (mut tx, mut ty) =
                    if ahead { (mid[j][0] - o[0], mid[j][1] - o[1]) } else { (o[0] - mid[j][0], o[1] - mid[j][1]) };
                let tl = {
                    let h = tx.hypot(ty);
                    if h == 0.0 { 1.0 } else { h }
                };
                tx /= tl;
                ty /= tl;
                let [nx, ny] = norm[j];
                let h = halfs[j];
                let mut out = Vec::new();
                for c in 1..CAP {
                    let t = (PI * c as f64) / CAP as f64;
                    // Ahead: from the + edge forward round to the - edge; behind: from the - edge back to the +.
                    let e = if ahead { t.cos() } else { -t.cos() };
                    let g = if ahead { t.sin() } else { -t.sin() };
                    out.push([mid[j][0] + (nx * e + tx * g) * h, mid[j][1] + (ny * e + ty * g) * h]);
                }
                out
            };
            // Runs of samples on one side, and at most a stretch long, each become a piece. Pieces overlap
            // by a sample where they meet, the one drawn first running on under the other, so no seam
            // shows: within a side the earlier piece runs on under the later; between sides the piece
            // behind the body (drawn before every piece in front) runs a sliver on under the piece in front.
            let mut from = 0;
            for j in 1..=last {
                let turns = j < last && near[j] != near[from];
                if j < last && !turns && j - from < STRETCH {
                    continue;
                }
                let to = j;
                let behind = !near[from];
                let end = if to < last && (!turns || behind) { to + 1 } else { to };
                let start = if from > 0 && behind && near[from - 1] { from - 1 } else { from };
                let mut pts: Vec<[f64; 2]> = Vec::new();
                for q in start..=end {
                    pts.push(edge(q, 1.0));
                }
                if end == last {
                    pts.extend(cap(last, true));
                }
                for q in (start..=end).rev() {
                    pts.push(edge(q, -1.0));
                }
                if start == 0 {
                    pts.extend(cap(0, false));
                }
                let x = pts.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
                let y = pts.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
                let xmax = pts.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
                let ymax = pts.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
                let d = format!(
                    "M{}Z",
                    pts.iter().map(|p| format!("{} {}", fmt(p[0]), fmt(p[1]))).collect::<Vec<_>>().join("L")
                );
                pieces.push(RibbonPiece {
                    d,
                    front: near[from],
                    bbox: Rect { x, y, w: xmax - x, h: ymax - y },
                    ribbon: i * LANES + l,
                    along: [ss[from], ss[to]],
                    from: mid[from],
                    to: mid[to],
                });
                from = j;
            }
        }
    }
    pieces
}

// ---------------------------------------------------------------------------
// Marks

/// One part of a mark: a fixed outline about its own centre, and where that centre is this frame.
#[derive(Debug, Clone)]
pub struct MarkPart {
    /// Stable name of the outline, e.g. "dot".
    pub shape: &'static str,
    pub d: String,
    pub bbox: Rect,
    /// Centre, viewBox units, then a rotation about the mark's centre (radians) and a scale about the part's own.
    pub x: f64,
    pub y: f64,
    pub turn: f64,
    pub scale: f64,
}

fn circle(r: f64) -> String {
    format!("M{} 0A{} {} 0 1 1 {} 0A{} {} 0 1 1 {} 0Z", fmt(r), fmt(r), fmt(r), fmt(-r), fmt(r), fmt(r), fmt(r))
}

fn capsule(w: f64, h: f64) -> String {
    let r = w / 2.0;
    let y = h / 2.0 - r;
    format!(
        "M{} {}A{} {} 0 0 1 {} {}L{} {}A{} {} 0 0 1 {} {}Z",
        fmt(-r),
        fmt(-y),
        fmt(r),
        fmt(r),
        fmt(r),
        fmt(-y),
        fmt(r),
        fmt(y),
        fmt(r),
        fmt(r),
        fmt(-r),
        fmt(y)
    )
}

/// The unit every mark is sized in: a share of the body's smaller side, within limits that read at any size.
fn mark_unit(b: &Bounds) -> f64 {
    clamp((b.max_x - b.min_x).min(b.max_y - b.min_y), 34.0, 78.0)
}

/// The parts of the marks at one moment. `bang` and `dots` 0..1 are how far the body has given
/// way to each (see `Pose`): a mark grows in as the body goes, from the body's centre.
/// The dots take turns hopping; the exclamation mark jumps and shakes every couple of seconds.
pub fn mark_parts(b: &Bounds, bang: f64, dots: f64, seed: u32, time: f64, still: bool) -> Vec<MarkPart> {
    let cx = (b.min_x + b.max_x) / 2.0;
    let cy = (b.min_y + b.max_y) / 2.0;
    let u = mark_unit(b);
    let t = if still { 0.0 } else { time + noise(seed, 9) * 10.0 };
    let mut parts: Vec<MarkPart> = Vec::new();
    // A mark starts to grow just before the body is gone, so something is always on screen.
    let grown = |weight: f64| smooth((clamp(weight, 0.0, 1.0) - 0.45) / 0.55);

    let gb = grown(bang);
    if gb > 0.0 {
        let (bar_w, bar_h, dot, gap) = (0.21 * u, 0.5 * u, 0.115 * u, 0.1 * u);
        let total = bar_h + gap + 2.0 * dot;
        // One alarm every 2.6 s: a jump, with a shake that dies away as it lands.
        let p = if still { 1.0 } else { ((t % 2.6) + 2.6) % 2.6 / 2.6 };
        let alarm = if p < 0.3 { p / 0.3 } else { 1.0 };
        let jump = if still { 0.0 } else { (PI * alarm).sin() };
        let turn = if still { 0.0 } else { 0.2 * (TAU * 3.0 * alarm).sin() * (1.0 - alarm) };
        let lift = -0.09 * u * jump;
        let bar_y = -total / 2.0 + bar_h / 2.0;
        let dot_y = total / 2.0 - dot;
        // The dot lags the bar on the way up, so the mark stretches as it jumps.
        parts.push(MarkPart {
            shape: "bar",
            d: capsule(bar_w, bar_h),
            bbox: Rect { x: -bar_w / 2.0, y: -bar_h / 2.0, w: bar_w, h: bar_h },
            x: cx,
            y: cy + (bar_y + lift) * gb,
            turn,
            scale: gb,
        });
        parts.push(MarkPart {
            shape: "dot",
            d: circle(dot),
            bbox: Rect { x: -dot, y: -dot, w: 2.0 * dot, h: 2.0 * dot },
            x: cx,
            y: cy + (dot_y + lift * 0.45) * gb,
            turn,
            scale: gb,
        });
    }

    let gd = grown(dots);
    if gd > 0.0 {
        let r = 0.1 * u;
        let step = 0.31 * u;
        for i in 0..3 {
            // Each dot hops in turn, a beat after the one before it.
            let p = if still { 0.0 } else { (((t - f64::from(i) * 0.17) % 1.25) + 1.25) % 1.25 / 1.25 };
            let hop = if p < 0.4 { (PI * (p / 0.4)).sin() } else { 0.0 };
            parts.push(MarkPart {
                shape: "dot3",
                d: circle(r),
                bbox: Rect { x: -r, y: -r, w: 2.0 * r, h: 2.0 * r },
                x: cx + f64::from(i - 1) * step * gd,
                y: cy - 0.16 * u * hop * gd,
                turn: 0.0,
                scale: gd * (1.0 + 0.12 * hop),
            });
        }
    }
    parts
}

/// How much of its size the body keeps while it gives way to a mark: gone a little past half way.
pub fn body_scale(bang: f64, dots: f64) -> f64 {
    1.0 - smooth(clamp(bang + dots, 0.0, 1.0) / 0.55)
}
