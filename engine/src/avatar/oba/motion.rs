//! Deterministic pose model (ported from `motion.ts`). A pose is a pure function of (seed, time,
//! state, mood, gaze, reduced_motion): no wall clock, no randomness, so any frame can be
//! reproduced exactly (tests, SSR, scrubbing).

use std::f64::consts::PI;

use super::color::js_round;
use super::types::{AvatarMood, AvatarMotion, AvatarMotionBlend, AvatarState, BlendFrom, Gaze, View};

/// How the eyes are drawn, as numbers that blend: any two looks mix into a shape in between, so
/// the eyes morph from one to the other (see `eyes.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EyeLook {
    /// 0 = the plain pill, 1 = the happy arc.
    pub arc: f64,
    /// 0 = the plain pill, 1 = the flat slit of a waiting bot. Never more than `1 - arc`.
    pub slit: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pose {
    /// View rotation in radians: yaw turns about the vertical axis, pitch about the horizontal.
    pub yaw: f64,
    pub pitch: f64,
    /// In-plane rotation, degrees.
    pub roll: f64,
    /// Vertical offset (negative = up), viewBox units.
    pub lift: f64,
    /// Squash and stretch about the base of the body.
    pub sx: f64,
    pub sy: f64,
    /// Eye look direction, -1..1 per axis.
    pub gaze_x: f64,
    pub gaze_y: f64,
    /// 0 = open, 1 = fully closed.
    pub blink: f64,
    /// Extra lid narrowing per eye, 0..1.
    pub squint_l: f64,
    pub squint_r: f64,
    pub eyes: EyeLook,
    /// Presence of the ribbons that circle the body: 0..1 draws them out and back in.
    pub ribbons: f64,
    /// Which layout the ribbons take: 0 a working turn's one hoop, 1 a celebration's cage of three.
    pub cage: f64,
    /// How far the body's turn has carried the ribbons round, radians, signed and not wrapped: each
    /// ribbon goes its own share of it, so a whole turn more does not put them back where they were.
    pub sweep: f64,
    /// How far the body has given way to a mark, 0..1 each: the exclamation mark and the three dots.
    pub bang: f64,
    pub dots: f64,
}

/// Every state and mood, in the order hosts list them.
pub const STATES: [AvatarState; 6] = [
    AvatarState::Idle,
    AvatarState::Thinking,
    AvatarState::Working,
    AvatarState::Waiting,
    AvatarState::Blocked,
    AvatarState::Done,
];
pub const MOODS: [AvatarMood; 2] = [AvatarMood::Neutral, AvatarMood::Happy];

impl AvatarState {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "idle" => Self::Idle,
            "thinking" => Self::Thinking,
            "working" => Self::Working,
            "waiting" => Self::Waiting,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            _ => return None,
        })
    }
}

impl AvatarMood {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "neutral" => Self::Neutral,
            "happy" => Self::Happy,
            _ => return None,
        })
    }
}

impl AvatarMotion {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "idle" => Self::Idle,
            "thinking" => Self::Thinking,
            "happy" => Self::Happy,
            _ => return None,
        })
    }
}

/// The state and mood to show. `motion` is the single option that came before them: `Thinking`
/// is a state, `Happy` a mood. An explicit `state` or `mood` wins over it.
pub fn resolve_look(
    state: Option<AvatarState>,
    mood: Option<AvatarMood>,
    motion: Option<AvatarMotion>,
) -> (AvatarState, AvatarMood) {
    (
        state.unwrap_or(if motion == Some(AvatarMotion::Thinking) { AvatarState::Thinking } else { AvatarState::Idle }),
        mood.unwrap_or(if motion == Some(AvatarMotion::Happy) { AvatarMood::Happy } else { AvatarMood::Neutral }),
    )
}

/// Resting view: turned a little and seen a little from above, so the volume reads without dominating.
pub const BASE_YAW: f64 = -0.3;
pub const BASE_PITCH: f64 = 0.2;
/// Following a look target: the body turns toward it by up to these angles
/// (radians, at full deflection) and the eyes use this share of their small
/// range, so the whole character looks rather than the eyes sliding alone.
pub const LOOK_YAW: f64 = 0.32;
pub const LOOK_PITCH: f64 = 0.18;
pub const LOOK_EYES: f64 = 0.7;

const TAU: f64 = PI * 2.0;

/// FNV-1a over the string's UTF-16 code units (as JS `charCodeAt` walks it).
pub fn hash_string(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for c in s.encode_utf16() {
        h ^= u32::from(c);
        h = h.wrapping_mul(16777619);
    }
    h
}

/// Stateless hash noise in [0, 1).
pub fn noise(seed: u32, n: i64) -> f64 {
    let mut h = seed ^ (n as i32 as u32).wrapping_mul(0x9e3779b1);
    h = (h ^ (h >> 16)).wrapping_mul(0x85ebca6b);
    h = (h ^ (h >> 13)).wrapping_mul(0xc2b2ae35);
    h ^= h >> 16;
    f64::from(h) / 4294967296.0
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    hi.min(lo.max(v))
}

fn smooth(u: f64) -> f64 {
    let x = clamp(u, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn mix(a: f64, b: f64, k: f64) -> f64 {
    a + (b - a) * k
}

/// Blink closure 0..1: one blink per ~4.2 s slot at a seeded offset, sometimes a double blink.
pub fn blink_at(seed: u32, t: f64) -> f64 {
    const SLOT: f64 = 4.2;
    const DUR: f64 = 0.19;
    let slot = (t / SLOT).floor() as i64;
    let mut closure = 0.0f64;
    for s in [slot - 1, slot] {
        let start = s as f64 * SLOT + 0.5 + noise(seed, s * 2) * 2.9;
        let starts = if noise(seed, s * 2 + 1) > 0.8 { vec![start, start + 0.34] } else { vec![start] };
        for b in starts {
            let u = (t - b) / DUR;
            if u > 0.0 && u < 1.0 {
                closure = closure.max((PI * u).sin());
            }
        }
    }
    closure
}

/// Idle eye wander: short holds between quick saccades, centered more often than not.
pub fn wander_at(seed: u32, t: f64) -> [f64; 2] {
    const SLOT: f64 = 1.9;
    let s = (t / SLOT).floor() as i64;
    let target = |k: i64| -> [f64; 2] {
        if noise(seed, 1000 + k * 3) < 0.4 {
            return [0.0, 0.0];
        }
        [(noise(seed, 1001 + k * 3) * 2.0 - 1.0) * 0.6, (noise(seed, 1002 + k * 3) * 2.0 - 1.0) * 0.35]
    };
    let a = target(s - 1);
    let b = target(s);
    let e = smooth((t - s as f64 * SLOT) / 0.16);
    [a[0] + (b[0] - a[0]) * e, a[1] + (b[1] - a[1]) * e]
}

/// Working: the eyes dart from one thing to the next, mostly down at the work.
fn dart_at(seed: u32, t: f64) -> [f64; 2] {
    const SLOT: f64 = 0.62;
    let s = (t / SLOT).floor() as i64;
    let target =
        |k: i64| -> [f64; 2] { [(noise(seed, 2001 + k * 3) * 2.0 - 1.0) * 0.8, 0.1 + noise(seed, 2002 + k * 3) * 0.5] };
    let a = target(s - 1);
    let b = target(s);
    let e = smooth((t - s as f64 * SLOT) / 0.12);
    [mix(a[0], b[0], e), mix(a[1], b[1], e)]
}

#[derive(Debug, Clone, Default)]
pub struct PoseInput {
    pub seed: u32,
    pub time: f64,
    pub state: Option<AvatarState>,
    pub mood: Option<AvatarMood>,
    /// Legacy; `state` and `mood` win when given.
    pub motion: Option<AvatarMotion>,
    /// Seconds since `state` began: `Done` celebrates from then on. Without it the state is at rest.
    pub state_age: Option<f64>,
    /// Look target, -1..1 per axis (x right, y down); `weight` 0..1 fades it in over the idle pose (default 1).
    pub gaze: Option<Gaze>,
    pub reduced_motion: bool,
    /// Extra view rotation in radians (e.g. from drag-to-turn), added to the resting view.
    pub view: Option<View>,
    /// Easing in from what was shown before (see `blend_poses`).
    pub blend: Option<AvatarMotionBlend>,
}

/// Pose part way from `a` to `b`, both taken at the same moment so either side's motion keeps
/// running. Everything is a number, the eyes' look included, so eyes morph from one shape to the
/// next and a body gives way to a mark (and back) part by part.
pub fn blend_poses(a: &Pose, b: &Pose, t: f64) -> Pose {
    let k = clamp(t, 0.0, 1.0);
    let m = |x: f64, y: f64| mix(x, y, k);
    // The ribbons stay where the side that shows them has carried them: mixing a celebration's many
    // turns with a pose that has none would wind them back round while they fade.
    let sweep = if a.ribbons > 0.0 && b.ribbons > 0.0 {
        m(a.sweep, b.sweep)
    } else if a.ribbons > 0.0 {
        a.sweep
    } else {
        b.sweep
    };
    // Likewise their layout: a celebration fading out keeps its cage.
    let cage = if a.ribbons > 0.0 && b.ribbons > 0.0 {
        m(a.cage, b.cage)
    } else if a.ribbons > 0.0 {
        a.cage
    } else {
        b.cage
    };
    Pose {
        yaw: m(a.yaw, b.yaw),
        pitch: m(a.pitch, b.pitch),
        roll: m(a.roll, b.roll),
        lift: m(a.lift, b.lift),
        sx: m(a.sx, b.sx),
        sy: m(a.sy, b.sy),
        gaze_x: m(a.gaze_x, b.gaze_x),
        gaze_y: m(a.gaze_y, b.gaze_y),
        blink: m(a.blink, b.blink),
        squint_l: m(a.squint_l, b.squint_l),
        squint_r: m(a.squint_r, b.squint_r),
        eyes: EyeLook { arc: m(a.eyes.arc, b.eyes.arc), slit: m(a.eyes.slit, b.eyes.slit) },
        ribbons: m(a.ribbons, b.ribbons),
        cage,
        sweep,
        bang: m(a.bang, b.bang),
        dots: m(a.dots, b.dots),
    }
}

pub fn compute_pose(input: &PoseInput) -> Pose {
    let pose = state_pose(input);
    let Some(blend) = &input.blend else { return pose };
    // A blend that is not a number (or is done) shows the pose as it is.
    if blend.t.is_nan() || blend.t >= 1.0 {
        return pose;
    }
    let (f_state, f_mood, f_motion, f_age, f_blend) = match &blend.from {
        BlendFrom::Motion(m) => (None, None, Some(*m), None, None),
        BlendFrom::Look(l) => (l.state, l.mood, l.motion, l.age, l.blend.as_deref().cloned()),
    };
    // A change of mood alone keeps the state's clock running on both sides, so a turn under way
    // stays one turn; a state that was left is held at the age it had.
    let state_age =
        if resolve_look(f_state, f_mood, f_motion).0 == resolve_look(input.state, input.mood, input.motion).0 {
            input.state_age
        } else {
            f_age
        };
    let before_input =
        PoseInput { state: f_state, mood: f_mood, motion: f_motion, state_age, blend: f_blend, ..input.clone() };
    let before = compute_pose(&before_input);
    blend_poses(&before, &pose, if blend.t.is_finite() { blend.t } else { 1.0 })
}

/// Done celebrates in cycles, timed as PR #14 measured the Grok Bot demo: the ribbons go round
/// DONE_SWEEPS times while the body turns DONE_SWEEPS / 3 times (see `celebration`), then they draw
/// back in and the bot rests. DONE_TURN_S covers the turn and the drawing in. A cycle lasts
/// DONE_CYCLE_S or up to a seventh longer, differently for each avatar, so several done bots on a
/// page drift apart instead of turning together.
pub const DONE_TURN_S: f64 = 3.87;
pub const DONE_CYCLE_S: f64 = 6.2;
const DONE_SWEEPS: f64 = 9.0;
/// How long the ribbons take to draw back in, seconds: they start once the turn has slowed below
/// about 0.9 rad/s, 3.37 s in, and are gone at DONE_TURN_S.
const DONE_DRAW_IN_S: f64 = 0.5;

/// A working bot gets on with it and every so often throws itself into the work: a band of
/// ribbons sweeps once round it, easing in like a spring settling, then draws back in. The first comes WORK_FIRST_S
/// (plus up to WORK_FIRST_JITTER_S) after the state begins, then one every WORK_GAP_S, each up to
/// WORK_JITTER_S late, so several working bots do not turn together. A turn takes WORK_TURN_S.
const WORK_FIRST_S: f64 = 1.2;
const WORK_FIRST_JITTER_S: f64 = 1.2;
const WORK_GAP_S: f64 = 7.5;
const WORK_JITTER_S: f64 = 1.5;
const WORK_TURN_S: f64 = 1.5;
/// How quickly a working turn eases in to its full circle, per second: a critically damped spring's rate.
const WORK_EASE: f64 = 6.2;

/// Where a working bot is in its turns `age` seconds into the state: how far into the current
/// turn (seconds), or None between turns.
fn work_turn(seed: u32, age: f64) -> Option<f64> {
    let first = WORK_FIRST_S + WORK_FIRST_JITTER_S * noise(seed, 41);
    if age < first {
        return None;
    }
    let n = ((age - first) / WORK_GAP_S).floor() as i64;
    // A turn may start up to WORK_JITTER_S late, so the one before may still be running.
    for k in [n, n - 1] {
        if k < 0 {
            continue;
        }
        let start = first + k as f64 * WORK_GAP_S + if k != 0 { WORK_JITTER_S * noise(seed, 60 + k) } else { 0.0 };
        if age >= start && age < start + WORK_TURN_S {
            return Some(age - start);
        }
    }
    None
}

/// An angle as the nearest way round to it, -π..π, so a blend never unwinds whole turns.
fn near(angle: f64) -> f64 {
    angle - TAU * js_round(angle / TAU)
}

/// How far a celebration's ribbons have gone round `t` seconds in, radians, on PR #14's curve: half
/// a radian of wind-up the other way over 0.24 s, a speed-up to a steady run, then from 2.54 s a
/// slow-down that lands, at 3.79 s, on DONE_SWEEPS whole turns.
const CELEBRATION_END_S: f64 = 3.79;
const CELEBRATION_SPEED: f64 = (DONE_SWEEPS * TAU + 0.5) / 2.4625;

pub fn celebration(t: f64) -> f64 {
    let v = CELEBRATION_SPEED;
    if t <= 0.0 {
        return 0.0;
    }
    if t < 0.24 {
        return -0.25 * (1.0 - (PI * t / 0.24).cos());
    }
    if t < 0.54 {
        return -0.5 + (v * (t - 0.24).powi(2)) / 0.6;
    }
    if t < 2.54 {
        return -0.5 + v * (t - 0.39);
    }
    if t < CELEBRATION_END_S {
        return -0.5 + v * 2.15 + (1.25 * v * (1.0 - (1.0 - (t - 2.54) / 1.25).powi(4))) / 4.0;
    }
    DONE_SWEEPS * TAU
}

fn state_pose(input: &PoseInput) -> Pose {
    let seed = input.seed;
    let time = input.time;
    let reduced = input.reduced_motion;
    let (state, mood) = resolve_look(input.state, input.mood, input.motion);
    // Per-avatar phase offset so several avatars on a page do not move in lockstep.
    let t = if reduced { 0.0 } else { time + noise(seed, 7) * 10.0 };
    let live = if reduced { 0.0 } else { 1.0 };
    let wave = |period: f64, phase: f64| if reduced { 0.0 } else { (TAU * t / period + phase).sin() };
    let given: Option<[f64; 2]> = input
        .gaze
        .filter(|g| g.x.is_finite() && g.y.is_finite())
        .map(|g| [clamp(g.x, -1.0, 1.0), clamp(g.y, -1.0, 1.0)]);
    let weight = match (given, input.gaze) {
        (Some(_), Some(g)) => match g.weight {
            Some(w) if w.is_finite() => clamp(w, 0.0, 1.0),
            _ => 1.0,
        },
        _ => 0.0,
    };
    // While it attends to a target the body holds its aim: idle sway of the turn settles to a trace.
    let sway = 1.0 - 0.7 * weight;

    let mut pose = Pose {
        yaw: BASE_YAW,
        pitch: BASE_PITCH,
        roll: 0.0,
        lift: 0.0,
        sx: 1.0,
        sy: 1.0,
        eyes: EyeLook { arc: if mood == AvatarMood::Happy { 1.0 } else { 0.0 }, slit: 0.0 },
        ..Pose::default()
    };

    // The calm pose of a bot with nothing to do: it breathes, sways, blinks and looks around.
    let rest = |pose: &mut Pose, pace: f64| {
        pose.yaw += 0.07 * sway * wave(6.1 * pace, 0.0);
        pose.pitch += 0.04 * sway * wave(4.7 * pace, 1.0);
        pose.roll = 1.6 * wave(5.3 * pace, 2.0);
        pose.lift = if reduced { 0.0 } else { -0.45 * (1.0 - (TAU * t / (3.4 * pace)).cos()) };
        let breath = wave(3.4 * pace, -PI / 2.0);
        pose.sy = 1.0 + 0.012 * breath;
        pose.sx = 1.0 - 0.008 * breath;
        if !reduced {
            pose.blink = blink_at(seed, t);
            let [gx, gy] = wander_at(seed, t);
            pose.gaze_x = gx;
            pose.gaze_y = gy;
        }
    };

    if state == AvatarState::Idle && mood == AvatarMood::Happy {
        // With nothing to do, a happy bot hops.
        let period = 0.92;
        let p = if reduced { 0.5 } else { ((t % period) + period) % period / period };
        let air = 4.0 * p * (1.0 - p);
        let land = if reduced { 0.0 } else { clamp(1.0 - p.min(1.0 - p) / 0.12, 0.0, 1.0) };
        pose.lift = if reduced { 0.0 } else { -3.2 * air };
        pose.sy = 1.0 - 0.09 * land + 0.03 * air * live;
        pose.sx = 1.0 + 0.07 * land - 0.015 * air * live;
        pose.roll = 4.5 * wave(period * 2.0, 0.0);
        pose.yaw += 0.1 * wave(period * 2.0, 0.6);
    } else if state == AvatarState::Thinking {
        // Looks up and away while it gives way to the three dots.
        pose.yaw += 0.12 + 0.05 * sway * wave(7.3, 0.0);
        pose.pitch += 0.08;
        pose.roll = -5.0 + 1.4 * wave(4.1, 0.0);
        pose.lift = -0.5 * (1.0 - (TAU * t / 5.2).cos()) * live;
        pose.gaze_x = 0.55 + 0.3 * wave(5.6, 0.0);
        pose.gaze_y = -0.8;
        pose.squint_l = 0.22;
        pose.squint_r = 0.32 + 0.1 * wave(3.1, 0.0);
        if !reduced {
            pose.blink = blink_at(seed, t * 0.7);
        }
        pose.dots = 1.0;
    } else if state == AvatarState::Working {
        // Bent over the work and busy: a quick steady bob and eyes darting across it, and every so
        // often a turn with a band of ribbons sweeping round it.
        let beat = wave(0.7, 0.0);
        pose.yaw += 0.08 * sway * wave(2.3, 0.4);
        pose.pitch -= 0.05;
        pose.roll = 3.0 + 0.6 * beat;
        pose.lift = -0.3 * (0.5 + 0.5 * beat) * live;
        pose.sy = 0.99 - 0.008 * beat;
        pose.sx = 1.005 + 0.005 * beat;
        pose.squint_l = 0.25;
        pose.squint_r = 0.25;
        if reduced {
            pose.gaze_x = 0.15;
            pose.gaze_y = 0.3;
        } else {
            let [gx, gy] = dart_at(seed, t);
            pose.gaze_x = gx;
            pose.gaze_y = gy;
            pose.blink = blink_at(seed, t * 1.3);
        }
        // Counted from when the state began; without an age, from the clock.
        let age = match input.state_age {
            Some(a) if a.is_finite() => a.max(0.0),
            _ => time.max(0.0),
        };
        let into = if reduced { None } else { work_turn(seed, age) };
        if let Some(into) = into {
            // A full circle that eases in like a spring settling. The band rides it round, out at the
            // start and back in at the end; the body keeps facing the work, leaning into the sweep, and
            // the eyes follow the band across.
            let e = WORK_EASE * into;
            let turn = TAU * (1.0 - (1.0 + e) * (-e).exp());
            let swell = (turn / 2.0).sin();
            pose.ribbons = clamp(into / 0.16, 0.0, 1.0) * clamp((WORK_TURN_S - into) / 0.5, 0.0, 1.0);
            pose.sweep = turn;
            pose.yaw += 0.22 * swell;
            pose.roll += 3.0 * swell;
            pose.lift -= 1.2 * swell;
            pose.gaze_x = clamp(pose.gaze_x * (1.0 - swell) - 0.85 * turn.sin(), -1.0, 1.0);
            pose.gaze_y = pose.gaze_y * (1.0 - swell) - 0.3 * swell;
        }
    } else if state == AvatarState::Waiting {
        // Leans to one side, eyes flattened to slits, glancing off at whatever it is waiting on.
        // Now and then it sighs. A happy bot keeps its arcs.
        let p = if reduced { 0.0 } else { ((t % 6.5) + 6.5) % 6.5 / 6.5 };
        // One breath in and out over the first third of the cycle.
        let sigh = (TAU * clamp(p / 0.33, 0.0, 1.0)).sin();
        pose.yaw += 0.04 * sway * wave(7.4, 0.0);
        pose.pitch += 0.02 + 0.03 * sigh;
        pose.roll = 7.0 + 1.2 * wave(6.4, 2.0);
        pose.lift = -0.3 * (1.0 - (TAU * t / 6.5).cos()) * live;
        pose.sy = 1.0 + 0.025 * sigh;
        pose.sx = 1.0 - 0.012 * sigh;
        pose.gaze_x = 0.5 + 0.15 * wave(8.2, 0.0);
        pose.gaze_y = 0.1;
        pose.eyes.slit = 1.0 - pose.eyes.arc;
        if !reduced {
            pose.blink = blink_at(seed, t * 0.6);
        }
    } else if state == AvatarState::Blocked {
        // Stops still while it gives way to the exclamation mark.
        pose.sy = 1.02;
        pose.sx = 0.99;
        pose.bang = 1.0;
    } else if state == AvatarState::Done {
        // Celebrates: a few turns with ribbons flying round it, then a rest, content and a little
        // slower than idle, and after a while again, the other way round.
        rest(&mut pose, 1.25);
        // Counted from when the state began. Without an age it is at rest.
        let age = match input.state_age {
            Some(a) if !reduced && a.is_finite() => a.max(0.0),
            _ => DONE_TURN_S,
        };
        let length = DONE_CYCLE_S * (1.0 + 0.15 * noise(seed, 31));
        let cycle = (age / length).floor();
        let since = age - cycle * length;
        if since < DONE_TURN_S {
            let swell = (PI * since / DONE_TURN_S).sin();
            let way = if (cycle as i64) % 2 != 0 { -1.0 } else { 1.0 };
            let sweep = celebration(since) * way;
            // The body turns a third as often as the ribbons go round.
            pose.yaw += near(sweep / 3.0);
            pose.lift -= 2.2 * swell;
            pose.sy += 0.04 * swell;
            // All the ribbons out at full strength while the turn is quick, drawn back in as it settles.
            pose.ribbons = clamp(since / 0.1, 0.0, 1.0) * clamp((DONE_TURN_S - since) / DONE_DRAW_IN_S, 0.0, 1.0);
            pose.cage = 1.0;
            // The ribbons only set off once the turn goes forward: the wind-up does not carry them.
            pose.sweep = way * celebration(since).max(0.0);
        }
    } else {
        rest(&mut pose, 1.0);
    }

    if let Some(given) = given
        && weight > 0.0
    {
        // A look target wins over idle wander. The body turns to face it: the resting
        // three-quarter turn gives way (so a target on either side turns the front that
        // way) and the eyes add a modest share on top. Reduced motion keeps the body
        // still, so the eyes alone use their full (still small) range. Happy arcs stay put.
        let share = if reduced { 1.0 } else { LOOK_EYES };
        let follow = weight * (1.0 - pose.eyes.arc);
        pose.gaze_x = mix(pose.gaze_x, given[0] * share, follow);
        pose.gaze_y = mix(pose.gaze_y, given[1] * share, follow);
        if !reduced {
            // Positive yaw turns the front to the right, positive pitch tips it up.
            pose.yaw += weight * (given[0] * LOOK_YAW - BASE_YAW);
            pose.pitch += weight * (-given[1] * LOOK_PITCH - BASE_PITCH / 2.0);
        }
    }
    if let Some(view) = input.view
        && view.yaw.is_finite()
        && view.pitch.is_finite()
    {
        pose.yaw += view.yaw;
        pose.pitch += view.pitch;
    }
    pose
}
