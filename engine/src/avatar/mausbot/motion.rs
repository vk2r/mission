//! How the cursor and the festival costumes move on the bar: the port of `mascot/motion.js`.
//!
//! The bar used to run these formulas in QML. They live here now and the engine publishes their
//! results (a pose per frame, and the blink and dwell numbers a row needs), so `BotRow.qml` imports
//! no JavaScript. Every number and formula is the original's; the tests are `scripts/check-motion.mjs`'s
//! checks, one by one.

use std::f64::consts::PI;

use serde::Serialize;

/// The cursor's body and the box it sits in, in picture units (`BODY`, `VIEW`, `BASE` of motion.js).
pub const BODY: f64 = 228.541;
pub const VIEW: f64 = BODY + 30.0;
pub const BASE: f64 = BODY;
const CENTRE: f64 = BODY / 2.0;

pub const BLINK_MS: f64 = 320.0;
pub const BLINK_SHUT: f64 = 0.42;
pub const BLINK_FLOOR: f64 = 0.04;
const SETTLE_MS: f64 = 1400.0;

/// One face's motion: `(amplitude, period in ms)` for each oscillation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    pub bob: Option<(f64, f64)>,
    pub circle: Option<(f64, f64)>,
    pub sway: Option<(f64, f64)>,
    pub pulse: Option<(f64, f64)>,
    pub jitter: Option<(f64, f64)>,
    pub enter: Option<(f64, f64)>,
    pub squash: Option<f64>,
    pub tilt: f64,
    pub settle: Option<f64>,
}

const NONE: Motion = Motion {
    bob: None,
    circle: None,
    sway: None,
    pulse: None,
    jitter: None,
    enter: None,
    squash: None,
    tilt: 0.0,
    settle: None,
};

/// `MOTION` (motion.js:8-48).
pub(crate) const MOTION: [(&str, Motion); 39] = [
    ("sleeping", Motion { pulse: Some((0.028, 4600.0)), tilt: 2.0, ..NONE }),
    ("waking", Motion { pulse: Some((0.03, 2200.0)), enter: Some((0.92, 700.0)), ..NONE }),
    ("idle", Motion { pulse: Some((0.014, 3600.0)), ..NONE }),
    ("listening", Motion { bob: Some((2.0, 2600.0)), pulse: Some((0.012, 2600.0)), ..NONE }),
    ("thinking", Motion { sway: Some((1.6, 3000.0)), pulse: Some((0.01, 3000.0)), ..NONE }),
    ("searching", Motion { bob: Some((3.0, 1400.0)), sway: Some((2.2, 1400.0)), ..NONE }),
    ("working", Motion { bob: Some((2.5, 900.0)), squash: Some(0.22), ..NONE }),
    ("excited", Motion { bob: Some((9.0, 520.0)), sway: Some((3.0, 1040.0)), squash: Some(0.35), ..NONE }),
    ("surprised", Motion { jitter: Some((0.8, 120.0)), enter: Some((1.14, 340.0)), ..NONE }),
    ("suspicious", Motion { sway: Some((2.4, 2600.0)), tilt: -3.0, ..NONE }),
    ("angry", Motion { jitter: Some((1.3, 95.0)), tilt: 2.0, ..NONE }),
    ("drowsy", Motion { pulse: Some((0.026, 5000.0)), tilt: 3.0, ..NONE }),
    ("happy", Motion { bob: Some((5.0, 820.0)), squash: Some(0.28), ..NONE }),
    ("curious", Motion { sway: Some((3.4, 1900.0)), tilt: -4.0, ..NONE }),
    ("confused", Motion { sway: Some((3.0, 2200.0)), ..NONE }),
    ("bored", Motion { pulse: Some((0.016, 5200.0)), tilt: 2.0, ..NONE }),
    ("proud", Motion { bob: Some((1.6, 2400.0)), pulse: Some((0.02, 2400.0)), ..NONE }),
    ("shy", Motion { pulse: Some((0.016, 3000.0)), tilt: 4.0, ..NONE }),
    ("sad", Motion { pulse: Some((0.02, 4600.0)), tilt: 3.0, ..NONE }),
    ("laughing", Motion { bob: Some((7.0, 430.0)), squash: Some(0.4), ..NONE }),
    ("scared", Motion { jitter: Some((2.2, 75.0)), ..NONE }),
    ("playful", Motion { bob: Some((6.0, 620.0)), sway: Some((5.0, 1240.0)), squash: Some(0.3), ..NONE }),
    ("celebrate", Motion { bob: Some((10.0, 480.0)), sway: Some((4.0, 960.0)), squash: Some(0.35), ..NONE }),
    ("orbit", Motion { circle: Some((6.0, 3200.0)), ..NONE }),
    ("radar", Motion { sway: Some((6.0, 2400.0)), pulse: Some((0.012, 2400.0)), ..NONE }),
    ("progress", Motion { pulse: Some((0.022, 1600.0)), ..NONE }),
    ("spawning", Motion { pulse: Some((0.014, 3600.0)), enter: Some((0.02, 820.0)), ..NONE }),
    ("humming", Motion { pulse: Some((0.016, 2800.0)), ..NONE }),
    ("loading", Motion { sway: Some((2.2, 1500.0)), pulse: Some((0.012, 1500.0)), ..NONE }),
    ("dictating", Motion { bob: Some((2.0, 2000.0)), ..NONE }),
    ("writing", Motion { bob: Some((1.6, 1100.0)), ..NONE }),
    ("sending", Motion { bob: Some((3.0, 900.0)), ..NONE }),
    ("receiving", Motion { bob: Some((3.0, 900.0)), ..NONE }),
    ("uploading", Motion { bob: Some((3.0, 1000.0)), ..NONE }),
    ("notifying", Motion { bob: Some((4.0, 700.0)), sway: Some((2.5, 700.0)), ..NONE }),
    ("alerting", Motion { jitter: Some((2.6, 85.0)), ..NONE }),
    ("dragging", Motion { sway: Some((2.0, 900.0)), tilt: -6.0, ..NONE }),
    ("bouncing", Motion { bob: Some((12.0, 560.0)), squash: Some(0.45), ..NONE }),
    ("powering-down", Motion { tilt: 4.0, settle: Some(0.05), ..NONE }),
];

/// `BLINK` (motion.js:50-59): `(shortest, longest)` pause between blinks, in ms.
pub(crate) const BLINK: [(&str, (f64, f64)); 23] = [
    ("idle", (6000.0, 14000.0)),
    ("listening", (3000.0, 7000.0)),
    ("thinking", (3500.0, 7000.0)),
    ("searching", (1600.0, 4000.0)),
    ("working", (2800.0, 5500.0)),
    ("excited", (2000.0, 4000.0)),
    ("surprised", (1800.0, 3500.0)),
    ("suspicious", (4500.0, 8000.0)),
    ("angry", (3500.0, 7000.0)),
    ("happy", (2500.0, 5000.0)),
    ("curious", (2500.0, 5500.0)),
    ("confused", (2800.0, 5500.0)),
    ("bored", (4000.0, 8000.0)),
    ("proud", (3500.0, 7000.0)),
    ("shy", (3000.0, 6000.0)),
    ("sad", (4000.0, 8000.0)),
    ("laughing", (2500.0, 5000.0)),
    ("scared", (1200.0, 3000.0)),
    ("playful", (2000.0, 4500.0)),
    ("celebrate", (2200.0, 4500.0)),
    ("humming", (4000.0, 8000.0)),
    ("notifying", (2000.0, 4000.0)),
    ("dragging", (2200.0, 4500.0)),
];

/// `FACE_DWELL` (motion.js:135-174): how long a face stays on one picture before it morphs to another.
pub(crate) const FACE_DWELL: [(&str, (f64, f64)); 39] = [
    ("sleeping", (6000.0, 10000.0)),
    ("waking", (800.0, 800.0)),
    ("idle", (9000.0, 16000.0)),
    ("listening", (2800.0, 5000.0)),
    ("thinking", (2000.0, 3600.0)),
    ("searching", (1000.0, 1800.0)),
    ("working", (1800.0, 3200.0)),
    ("excited", (1100.0, 2000.0)),
    ("surprised", (2500.0, 4000.0)),
    ("suspicious", (2600.0, 4500.0)),
    ("angry", (2200.0, 3800.0)),
    ("drowsy", (4000.0, 8000.0)),
    ("happy", (2500.0, 4500.0)),
    ("curious", (1800.0, 3200.0)),
    ("confused", (2200.0, 3800.0)),
    ("bored", (3500.0, 6000.0)),
    ("proud", (3500.0, 6000.0)),
    ("shy", (3000.0, 5500.0)),
    ("sad", (4000.0, 7000.0)),
    ("laughing", (1200.0, 2400.0)),
    ("scared", (900.0, 1800.0)),
    ("playful", (1500.0, 3000.0)),
    ("celebrate", (1400.0, 2600.0)),
    ("orbit", (4000.0, 8000.0)),
    ("radar", (4000.0, 8000.0)),
    ("progress", (4000.0, 8000.0)),
    ("spawning", (1200.0, 1200.0)),
    ("humming", (5000.0, 9000.0)),
    ("loading", (6000.0, 10000.0)),
    ("dictating", (4000.0, 8000.0)),
    ("sending", (4000.0, 8000.0)),
    ("receiving", (4000.0, 8000.0)),
    ("uploading", (4000.0, 8000.0)),
    ("writing", (4000.0, 8000.0)),
    ("notifying", (1500.0, 2600.0)),
    ("alerting", (2000.0, 3600.0)),
    ("bouncing", (3000.0, 6000.0)),
    ("dragging", (1600.0, 3000.0)),
    ("powering-down", (6000.0, 9000.0)),
];

/// `motionOf` (motion.js:66): a face the table leaves out does not move.
pub fn motion_of(face: &str) -> Motion {
    MOTION.iter().find(|(name, _)| *name == face).map_or(NONE, |(_, motion)| *motion)
}

/// Where the picture is, as the bar draws it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub dx: f64,
    pub dy: f64,
    pub rot: f64,
    pub u: f64,
    pub sx: f64,
    pub sy: f64,
}

/// The rest pose: where the picture sits when nothing moves it.
#[cfg(test)]
pub const REST: Pose = Pose { dx: 0.0, dy: 0.0, rot: 0.0, u: 1.0, sx: 1.0, sy: 1.0 };

fn ease(e: f64) -> f64 {
    let a = e - 1.0;
    1.0 + (1.7 + 1.0) * a * a * a + 1.7 * a * a
}

fn settle_ease(e: f64) -> f64 {
    if e < 0.5 { 2.0 * e * e } else { 1.0 - 2.0 * (1.0 - e) * (1.0 - e) }
}

/// `pose(face, t)` (motion.js:71-103), `t` in ms since the face began.
pub fn pose(face: &str, t: f64) -> Pose {
    let m = motion_of(face);
    let (mut s, mut c, mut d, mut u, mut sx, mut sy) = (0.0, 0.0, m.tilt, 1.0, 1.0, 1.0);
    let r = |period: f64, phase: f64| (t / period * PI * 2.0 + phase).sin();
    if let Some((amplitude, period)) = m.bob {
        let w = r(period, 0.0);
        c -= amplitude * w;
        if let Some(squash) = m.squash.filter(|squash| *squash != 0.0) {
            let q = squash * f64::max(0.0, -w);
            sy = 1.0 - q * 0.5;
            sx = 1.0 + q * 0.5;
        }
    }
    if let Some((radius, period)) = m.circle {
        s += radius * r(period, 0.0);
        c += radius * r(period, PI / 2.0);
    }
    if let Some((degrees, period)) = m.sway {
        d += degrees * r(period, 0.0);
    }
    if let Some((amount, period)) = m.pulse {
        u *= 1.0 + amount * r(period, 0.0);
    }
    if let Some((amplitude, period)) = m.jitter {
        s += amplitude * r(period, 0.0);
        c += amplitude * r(period * 0.63, 1.1);
    }
    if let Some((from, ms)) = m.enter {
        let k = t / ms;
        u *= if k >= 1.0 { 1.0 } else { from + (1.0 - from) * ease(f64::max(k, 0.0)) };
    }
    if let Some(to) = m.settle {
        let v = (t / SETTLE_MS).clamp(0.0, 1.0);
        u *= 1.0 + (to - 1.0) * settle_ease(v);
    }
    Pose { dx: s, dy: c, rot: d, u, sx, sy }
}

/// `frameMs(face, size)` (motion.js:105-122): how often a face needs a new pose at `size` pixels, 0 when
/// it is too small to see move.
pub fn frame_ms(face: &str, size: f64) -> f64 {
    let m = motion_of(face);
    let a = size / VIEW;
    let rp = |amplitude: f64, period: f64| {
        if amplitude * a >= 0.2 { PI * 2.0 * amplitude * a / period } else { 0.0 }
    };
    let mut n = 0.0;
    if let Some((amplitude, period)) = m.bob {
        n += rp(amplitude, period);
        if let Some(squash) = m.squash.filter(|squash| *squash != 0.0) {
            n += rp(squash * 0.5 * BODY, period);
        }
    }
    if let Some((degrees, period)) = m.sway {
        n += rp(degrees * PI / 180.0 * CENTRE, period);
    }
    if let Some((amount, period)) = m.pulse {
        n += rp(amount * CENTRE, period);
    }
    if let Some((radius, period)) = m.circle {
        n += rp(radius, period);
    }
    if let Some((amplitude, period)) = m.jitter {
        n += rp(amplitude, period * 0.63);
    }
    if n.is_nan() || n <= 0.0 {
        return 0.0;
    }
    let steps = [1000.0 / 20.0, 1000.0 / 30.0, 1000.0 / 60.0];
    steps.into_iter().find(|step| n * step <= 0.5).unwrap_or(1000.0 / 60.0)
}

/// `blinkOpen(t)` (motion.js:124-128): how open the eyes are `t` ms into a blink.
#[cfg(test)]
pub fn blink_open(t: f64) -> f64 {
    let a = t / BLINK_MS;
    if a >= 1.0 {
        return 1.0;
    }
    f64::max(if a < BLINK_SHUT { 1.0 - a / BLINK_SHUT } else { (a - BLINK_SHUT) / (1.0 - BLINK_SHUT) }, BLINK_FLOOR)
}

/// The range a face's pause between blinks falls in; `None` for a face that never blinks.
pub fn blink_range(face: &str) -> Option<(f64, f64)> {
    BLINK.iter().find(|(name, _)| *name == face).map(|(_, range)| *range)
}

/// `blinkPause(face, unit)` (motion.js:130-133): `unit` in 0..1 picks a point of the range.
#[cfg(test)]
pub fn blink_pause(face: &str, unit: f64) -> Option<f64> {
    blink_range(face).map(|(low, high)| low + unit * (high - low))
}

/// The range of `FACE_DWELL`, idle's for a face it leaves out.
pub fn dwell_range(face: &str) -> (f64, f64) {
    let find = |wanted: &str| FACE_DWELL.iter().find(|(name, _)| *name == wanted).map(|(_, range)| *range);
    find(face).or_else(|| find("idle")).unwrap_or((9000.0, 16000.0))
}

/// `dwellMs(face, unit)` (motion.js:176-179).
#[cfg(test)]
pub fn dwell_ms(face: &str, unit: f64) -> f64 {
    let (low, high) = dwell_range(face);
    low + unit * (high - low)
}

/// What a row carries so the bar can blink, morph and squash a picture without the formulas: the
/// numbers of `blinkPause`, `dwellMs` and the blink's shape, and where the squash pivots.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Published {
    /// `FACE_DWELL`'s range for the face, ms: the bar picks a point of it for each morph.
    pub dwell: [f64; 2],
    /// `BLINK`'s range for the face, ms; absent for a face that never blinks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blink: Option<[f64; 2]>,
    #[serde(rename = "blinkMs")]
    pub blink_ms: f64,
    #[serde(rename = "blinkShut")]
    pub blink_shut: f64,
    #[serde(rename = "blinkFloor")]
    pub blink_floor: f64,
    /// Where the squash pivots, as a fraction of the picture's height (`(BASE + 15) / VIEW`).
    #[serde(rename = "squashOrigin")]
    pub squash_origin: f64,
}

pub fn published(face: &str) -> Published {
    let (low, high) = dwell_range(face);
    Published {
        dwell: [low, high],
        blink: blink_range(face).map(|(low, high)| [low, high]),
        blink_ms: BLINK_MS,
        blink_shut: BLINK_SHUT,
        blink_floor: BLINK_FLOOR,
        squash_origin: (BASE + 15.0) / VIEW,
    }
}

/// A pose in the units the bar applies it in: the shift as a fraction of the picture (the bar's
/// `p.dx * width / VIEW`), the rest as they are.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Frame {
    pub dx: f64,
    pub dy: f64,
    pub rot: f64,
    pub u: f64,
    pub sx: f64,
    pub sy: f64,
}

pub fn frame(face: &str, t: f64) -> Frame {
    let p = pose(face, t);
    Frame { dx: p.dx / VIEW, dy: p.dy / VIEW, rot: p.rot, u: p.u, sx: p.sx, sy: p.sy }
}
