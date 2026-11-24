//! Ported from upstream `states.test.ts`. Not ported: "show in the scene too" (`sceneFrame`) and the
//! whole "animator states" suite (`animator.ts`), which are not part of this port. MausBot's own
//! behaviour (the comet palettes and the mouth) replaces upstream's "ramp of the body's own color"
//! test, see `ribbons_take_the_mausbot_comet_palettes_whatever_the_body_color`.

use super::*;
use crate::avatar::oba::effects::{LANES, ribbon_pieces, ribbon_reach};
use crate::avatar::oba::mood::MOOD_MS;
use crate::avatar::oba::motion::{
    DONE_CYCLE_S, DONE_TURN_S, MOODS, PoseInput, STATES, blend_poses, compute_pose, hash_string, resolve_look,
};
use crate::avatar::oba::path::is_inside;
use crate::avatar::oba::render::{avatar_extent, prepared_volume, ribbon_rims, ribbons_follow};
use crate::avatar::oba::types::{AvatarMotionBlend, BlendFrom, BlendLook};
use crate::avatar::oba::validate::analyze_body;
use crate::avatar::oba::volume::edge_distance;
use std::f64::consts::{PI, TAU};

fn mug() -> AvatarDefinition {
    ex("mug")
}

fn eased_from(state: AvatarState, mood: Option<AvatarMood>, t: f64) -> Option<AvatarMotionBlend> {
    Some(AvatarMotionBlend { from: BlendFrom::Look(BlendLook { state: Some(state), mood, ..BlendLook::default() }), t })
}

fn eyes(d: &AvatarDefinition, o: RenderOptions) -> Vec<EyeMark> {
    eyes_of(&frame(d, &o)).into_iter().cloned().collect()
}

fn is_pill(m: &EyeMark) -> bool {
    matches!(m, EyeMark::Pill { .. })
}
fn is_arc(m: &EyeMark) -> bool {
    matches!(m, EyeMark::Arc { .. })
}
fn is_path(m: &EyeMark) -> bool {
    matches!(m, EyeMark::Path { .. })
}

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b} (tolerance {tol})");
}

fn min(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::INFINITY, f64::min)
}
fn max(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
}

/// states.test.ts "lists the six states and the moods, and still reads the single option that came
/// before them". The invalid inputs ('sad', 'nope', 7) are text upstream: here they are `parse`
/// returning `None`, which `resolve_look` then defaults.
#[test]
fn state_and_mood_list_the_six_states_and_read_the_single_option_before_them() {
    assert_eq!(
        STATES,
        [
            AvatarState::Idle,
            AvatarState::Thinking,
            AvatarState::Working,
            AvatarState::Waiting,
            AvatarState::Blocked,
            AvatarState::Done
        ]
    );
    assert!(MOODS.contains(&AvatarMood::Happy));
    let (i, n, h) = (AvatarState::Idle, AvatarMood::Neutral, AvatarMood::Happy);
    assert_eq!(resolve_look(None, None, None), (i, n));
    assert_eq!(resolve_look(None, None, Some(AvatarMotion::Thinking)), (AvatarState::Thinking, n));
    assert_eq!(resolve_look(None, None, Some(AvatarMotion::Happy)), (i, h));
    assert_eq!(
        resolve_look(Some(AvatarState::Working), Some(h), Some(AvatarMotion::Thinking)),
        (AvatarState::Working, h)
    );
    assert_eq!(resolve_look(AvatarState::parse("nope"), AvatarMood::parse("sad"), None), (i, n));
    assert_eq!(AvatarState::parse("done"), Some(AvatarState::Done));
}

/// states.test.ts "draws the old motions exactly as the state and mood they mean".
#[test]
fn state_and_mood_draw_the_old_motions_as_the_state_and_mood_they_mean() {
    for d in examples() {
        for time in [0.0, 0.7, 2.3] {
            let o = |m, s, mo| RenderOptions { motion: m, state: s, mood: mo, time: Some(time), ..opts() };
            assert_eq!(
                svg(&d, &o(Some(AvatarMotion::Happy), None, None)),
                svg(&d, &o(None, Some(AvatarState::Idle), Some(AvatarMood::Happy)))
            );
            assert_eq!(svg(&d, &o(Some(AvatarMotion::Idle), None, None)), svg(&d, &o(None, None, None)));
            assert_eq!(
                svg(&d, &o(Some(AvatarMotion::Thinking), None, None)),
                svg(&d, &o(None, Some(AvatarState::Thinking), None))
            );
        }
    }
}

/// states.test.ts "keeps a mood on any state: a happy bot has its arcs while it works, waits and
/// finishes".
#[test]
fn state_and_mood_keep_a_mood_on_any_state() {
    for state in [AvatarState::Idle, AvatarState::Working, AvatarState::Waiting, AvatarState::Done] {
        let marks = eyes(
            &mug(),
            RenderOptions {
                state: Some(state),
                mood: Some(AvatarMood::Happy),
                time: Some(1.1),
                state_age: Some(DONE_TURN_S + 1.0),
                ..opts()
            },
        );
        assert_eq!(marks.len(), 2);
        assert!(marks.iter().all(is_arc), "{state:?}");
    }
}

/// states.test.ts "are the plain pill and the plain arc at rest, and an outline in between".
#[test]
fn eyes_are_the_plain_pill_and_the_plain_arc_at_rest_and_an_outline_in_between() {
    let blended = |t: f64| {
        eyes(
            &mug(),
            RenderOptions {
                mood: Some(AvatarMood::Happy),
                time: Some(1.1),
                motion_blend: eased_from(AvatarState::Idle, Some(AvatarMood::Neutral), t),
                ..opts()
            },
        )
    };
    assert!(eyes(&mug(), at(1.1)).iter().all(is_pill));
    assert!(blended(1.0).iter().all(is_arc));
    for t in [0.1, 0.5, 0.9] {
        assert!(blended(t).iter().all(is_path), "t = {t}");
    }
}

/// The numbers of an outline mark (empty for any other mark).
fn outline(t: f64) -> Vec<f64> {
    let marks = eyes(
        &mug(),
        RenderOptions {
            mood: Some(AvatarMood::Happy),
            reduced_motion: true,
            motion_blend: eased_from(AvatarState::Idle, None, t),
            ..opts()
        },
    );
    match &marks[0] {
        EyeMark::Path { d } => numbers(d),
        _ => vec![],
    }
}

/// states.test.ts "morph without a jump: the outline starts at the pill and each step moves a little".
#[test]
fn eyes_morph_without_a_jump() {
    let pill = match &eyes(&mug(), still())[0] {
        EyeMark::Pill { x, y, width, height, .. } => (*x, *y, *width, *height),
        other => panic!("expected a pill, got {other:?}"),
    };
    let start = outline(0.001);
    let xs: Vec<f64> = start.iter().step_by(2).cloned().collect();
    let ys: Vec<f64> = start.iter().skip(1).step_by(2).cloned().collect();
    near(min(&xs), pill.0, 0.05);
    near(max(&xs), pill.0 + pill.2, 0.05);
    near(min(&ys), pill.1, 0.05);
    near(max(&ys), pill.1 + pill.3, 0.05);
    let mut last = start;
    let mut t = 0.05;
    while t < 1.0 {
        let next = outline(t);
        assert_eq!(next.len(), last.len());
        let step = next.iter().zip(&last).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        assert!(step < 1.2, "t = {t}: step {step}");
        last = next;
        t += 0.05;
    }
}

/// states.test.ts "stay inside the body for every mood, state and look direction, on every starter
/// and gallery avatar".
#[test]
fn eyes_stay_inside_the_body_for_every_mood_state_and_look_direction() {
    let looks: [Option<Gaze>; 7] =
        [None, gaze(1.0, 0.0), gaze(-1.0, 0.0), gaze(0.0, 1.0), gaze(0.0, -1.0), gaze(1.0, 1.0), gaze(-1.0, -1.0)];
    let all = everyone();
    assert!(all.len() > examples().len());
    for d in &all {
        let polys = analyze_body(&prepared_volume(d, None).unwrap().0.body_path).unwrap().polys;
        for mood in MOODS {
            for state in [AvatarState::Idle, AvatarState::Waiting] {
                for g in looks {
                    let other = if state == AvatarState::Idle { AvatarState::Waiting } else { AvatarState::Idle };
                    let plain =
                        RenderOptions { state: Some(state), mood: Some(mood), gaze: g, time: Some(0.9), ..opts() };
                    let blended = RenderOptions {
                        motion_blend: eased_from(other, Some(AvatarMood::Happy), 0.5),
                        ..plain.clone()
                    };
                    for o in [plain, blended] {
                        let mut outside = 0;
                        for mark in eyes(d, o) {
                            let (pts, step, from) = match &mark {
                                EyeMark::Pill { x, y, width, height, .. } => {
                                    (vec![*x, *y, x + width, y + height, x + width / 2.0, y + height / 2.0], 2, 0)
                                }
                                // An outline's numbers are a start point, then a control point and a midpoint per step:
                                // the control points are the outline itself, and the curve lies inside their hull.
                                EyeMark::Path { d } => (numbers(d), 4, 2),
                                EyeMark::Arc { .. } => (vec![], 2, 0),
                            };
                            let mut i = from;
                            while i + 1 < pts.len() {
                                // The pill's corners are rounded off; test them a little inside.
                                let inset =
                                    if is_pill(&mark) && i < 4 { (if i == 0 { 1.0 } else { -1.0 }) * 0.6 } else { 0.0 };
                                if !is_inside(&polys, pts[i] + inset, pts[i + 1] + inset) {
                                    outside += 1;
                                }
                                i += step;
                            }
                        }
                        assert_eq!(outside, 0, "{} {state:?} {mood:?} {g:?}", d.id);
                    }
                }
            }
        }
    }
}

/// states.test.ts "flatten to parallel slits while waiting, unless the bot is happy".
#[test]
fn eyes_flatten_to_parallel_slits_while_waiting_unless_the_bot_is_happy() {
    let waiting = RenderOptions { state: Some(AvatarState::Waiting), ..still() };
    let boxes: Vec<(f64, f64, f64)> = eyes(&mug(), waiting.clone())
        .iter()
        .map(|m| {
            let EyeMark::Path { d } = m else { panic!("expected an outline") };
            let n = numbers(d);
            let xs: Vec<f64> = n.iter().step_by(2).cloned().collect();
            let ys: Vec<f64> = n.iter().skip(1).step_by(2).cloned().collect();
            // The slope of the slit: where its left end and its right end sit.
            let at_x = |x: f64| ys[xs.iter().position(|v| *v == x).unwrap()];
            (max(&xs) - min(&xs), max(&ys) - min(&ys), at_x(max(&xs)) - at_x(min(&xs)))
        })
        .collect();
    assert_eq!(boxes.len(), 2);
    for (w, h, _) in &boxes {
        assert!(*w > 2.0 * h);
    }
    assert!(boxes[0].2 > 0.2);
    near(boxes[1].2, boxes[0].2, 0.05);
    assert!(eyes(&mug(), RenderOptions { mood: Some(AvatarMood::Happy), ..waiting }).iter().all(is_arc));
}

/// states.test.ts "replace the body: three dots while thinking, an exclamation mark while blocked".
#[test]
fn marks_replace_the_body_three_dots_while_thinking_an_exclamation_mark_while_blocked() {
    for d in examples() {
        let thinking = frame(&d, &state_at(AvatarState::Thinking, 0.8));
        assert_eq!(thinking.shapes.iter().filter(|s| is_mark(s)).count(), 3, "{}", d.id);
        assert_eq!(thinking.shapes.iter().filter(|s| is_body(s)).count(), 0);
        assert_eq!(eyes_of(&thinking).len(), 0);
        let blocked = frame(&d, &state_at(AvatarState::Blocked, 0.8));
        assert_eq!(blocked.shapes.iter().filter(|s| is_mark(s)).count(), 2);
        assert_eq!(blocked.shapes.iter().filter(|s| is_body(s)).count(), 0);
        // Every mark's path is listed once, under an id of its own.
        for s in thinking.shapes.iter().chain(&blocked.shapes) {
            if let FrameShape::Slice { path, .. } = s {
                assert!(thinking.paths.iter().chain(&blocked.paths).any(|p| p.id == *path));
            }
        }
    }
}

/// states.test.ts "grow in as the body shrinks away, and something is on screen all the way".
#[test]
fn marks_grow_in_as_the_body_shrinks_away() {
    let blend = |t: f64| RenderOptions {
        state: Some(AvatarState::Blocked),
        time: Some(1.0),
        motion_blend: eased_from(AvatarState::Idle, None, t),
        ..opts()
    };
    for i in 0..=20 {
        let t = i as f64 * 0.05;
        let f = frame(&mug(), &blend(t));
        assert!(!f.shapes.is_empty());
        if t < 0.3 - 1e-9 {
            assert_eq!(f.shapes.iter().filter(|s| is_mark(s)).count(), 0, "t = {t}");
        }
        if t > 0.7 {
            assert_eq!(f.shapes.iter().filter(|s| is_body(s)).count(), 0, "t = {t}");
        }
    }
    let scale = |f: &AvatarFrame| matrix_of(f.shapes.iter().find(|s| is_body(s)).unwrap())[3].abs();
    assert!(scale(&frame(&mug(), &blend(0.3))) < scale(&frame(&mug(), &at(1.0))));
}

/// states.test.ts "hold still under reduced motion and move otherwise".
#[test]
fn marks_hold_still_under_reduced_motion_and_move_otherwise() {
    let shapes = |time: f64, reduced: bool| {
        format!(
            "{:?}",
            frame(
                &mug(),
                &RenderOptions {
                    state: Some(AvatarState::Thinking),
                    time: Some(time),
                    reduced_motion: reduced,
                    ..opts()
                }
            )
            .shapes
        )
    };
    assert_eq!(shapes(0.2, true), shapes(1.4, true));
    assert_ne!(shapes(0.2, false), shapes(0.5, false));
}

fn ribbons(f: &AvatarFrame) -> Vec<&FrameShape> {
    ribbons_of(f)
}

fn working_pose(d: &AvatarDefinition, age: f64) -> crate::avatar::oba::motion::Pose {
    compute_pose(&PoseInput {
        seed: hash_string(&d.id),
        time: age,
        state: Some(AvatarState::Working),
        state_age: Some(age),
        ..PoseInput::default()
    })
}

/// states.test.ts "sweep round a working bot every few seconds, behind the body and in front of it".
#[test]
fn ribbons_sweep_round_a_working_bot_every_few_seconds_behind_and_in_front_of_the_body() {
    let m = mug();
    // Over half a minute of working: when the band is out.
    let out: Vec<f64> = (0..1500).map(|i| i as f64 * 0.02).filter(|age| working_pose(&m, *age).ribbons > 0.0).collect();
    let starts: Vec<f64> =
        out.iter().enumerate().filter(|(i, a)| *i == 0 || *a - out[i - 1] > 0.03).map(|(_, a)| *a).collect();
    let ends: Vec<f64> = out
        .iter()
        .enumerate()
        .filter(|(i, a)| *i == out.len() - 1 || out[i + 1] - *a > 0.03)
        .map(|(_, a)| *a)
        .collect();
    // The first sweep comes 1.2 to 2.4 seconds in, then one every six to nine seconds, each over in a second and a half.
    assert!(starts[0] > 1.1 && starts[0] < 2.5, "first sweep at {}", starts[0]);
    assert!(starts.len() >= 4, "{starts:?}");
    for (i, s) in starts.iter().enumerate().skip(1) {
        let gap = s - starts[i - 1];
        assert!(gap > 5.9 && gap < 9.1, "gap {gap}");
    }
    for (i, s) in starts.iter().enumerate() {
        assert!(ends[i] - s < 1.55);
    }
    // During a sweep the band passes behind the body and in front of it, painted in that order.
    let (mut front, mut back) = (0, 0);
    for i in 0..20 {
        let age = starts[0] + 0.05 + i as f64 * 0.05;
        let f = frame(
            &m,
            &RenderOptions { state: Some(AvatarState::Working), time: Some(age), state_age: Some(age), ..opts() },
        );
        let all = ribbons(&f);
        assert!(!all.is_empty());
        let is_front = |s: &&FrameShape| matches!(s, FrameShape::Ribbon { front: true, .. });
        front += all.iter().filter(|s| is_front(s)).count();
        back += all.iter().filter(|s| !is_front(s)).count();
        let body: Vec<usize> = f
            .shapes
            .iter()
            .enumerate()
            .filter(|(_, s)| !matches!(s, FrameShape::Ribbon { .. }))
            .map(|(at, _)| at)
            .collect();
        let (first, last_body) = (body[0], *body.last().unwrap());
        for (at, s) in f.shapes.iter().enumerate() {
            if let FrameShape::Ribbon { front, .. } = s {
                assert!(if *front { at > last_body } else { at < first });
            }
        }
    }
    assert!(front > 0 && back > 0);
    // Between sweeps, at rest, in any other state at rest and under reduced motion there are none.
    assert_eq!(
        ribbons(&frame(
            &m,
            &RenderOptions { state: Some(AvatarState::Working), time: Some(0.5), state_age: Some(0.5), ..opts() }
        ))
        .len(),
        0
    );
    for state in
        [AvatarState::Idle, AvatarState::Thinking, AvatarState::Waiting, AvatarState::Blocked, AvatarState::Done]
    {
        let f = frame(
            &m,
            &RenderOptions { state: Some(state), time: Some(1.0), state_age: Some(DONE_TURN_S + 0.5), ..opts() },
        );
        assert_eq!(ribbons(&f).len(), 0, "{state:?}");
    }
    let during = starts[0] + 0.6;
    let f = frame(
        &m,
        &RenderOptions {
            state: Some(AvatarState::Working),
            time: Some(during),
            state_age: Some(during),
            reduced_motion: true,
            ..opts()
        },
    );
    assert_eq!(ribbons(&f).len(), 0);
}

/// MausBot's ribbon palettes, written out from `render.ts` `COMET_COLORS` (lower-cased, as the
/// renderer writes hex): three neighbouring hues head to tail.
const COMET: [[&str; 3]; 6] = [
    ["#a855f7", "#6366f1", "#38bdf8"],
    ["#22d3ee", "#34d399", "#a3e635"],
    ["#fb923c", "#f43f5e", "#d946ef"],
    ["#818cf8", "#c084fc", "#f472b6"],
    ["#facc15", "#fb923c", "#ec4899"],
    ["#34d399", "#22d3ee", "#60a5fa"],
];

/// How far a hex colour is from the nearest point of a palette's two-segment run, in 0..255 units.
fn off_run(hex: &str, row: &[&str; 3]) -> f64 {
    let rgb = |h: &str| -> [f64; 3] { [1, 3, 5].map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap() as f64) };
    let p = rgb(hex);
    let mut best = f64::INFINITY;
    for w in row.windows(2) {
        let (a, b) = (rgb(w[0]), rgb(w[1]));
        let d: Vec<f64> = (0..3).map(|k| b[k] - a[k]).collect();
        let t = ((0..3).map(|k| (p[k] - a[k]) * d[k]).sum::<f64>() / (0..3).map(|k| d[k] * d[k]).sum::<f64>())
            .clamp(0.0, 1.0);
        best = best.min((0..3).map(|k| (p[k] - (a[k] + t * d[k])).powi(2)).sum::<f64>().sqrt());
    }
    best
}

/// MausBot replaces upstream's "are steps along a ramp of the body's own color…" (the ribbon colour
/// follows the body there). Here a ribbon's gradient stops are sampled from one of the six comet
/// palettes (a piece covers a stretch of its run, so the stops lie on the run, within the hex
/// rounding), its plainest colour is the middle stop, the colours do not depend on the body's colour,
/// and a piece continues the colour of the one before it where they meet (the continuity half of the
/// upstream test).
#[test]
fn ribbons_take_the_mausbot_comet_palettes_whatever_the_body_color() {
    let colors = ["#1f7cf2", "#ff6a1a", "#f8a01c", "#f2384b", "#a8693f", "#7a7a7a", "#f4f4f4", "#151515"];
    let ribbons_at = |color: &str, i: usize| {
        let o = RenderOptions {
            state: Some(AvatarState::Done),
            time: Some(i as f64 * 0.17),
            state_age: Some(0.6 + i as f64 * 0.12),
            color: Some(color.into()),
            ..opts()
        };
        frame(&mug(), &o).shapes.into_iter().filter(|s| matches!(s, FrameShape::Ribbon { .. })).collect::<Vec<_>>()
    };
    for color in colors {
        for i in 0..16 {
            let shapes = ribbons_at(color, i);
            assert!(!shapes.is_empty());
            // Not tied to the body's colour: the same moment in another colour has the same ribbons.
            assert_eq!(shapes, ribbons_at("#ffffff", i), "{color}");
            for s in &shapes {
                let FrameShape::Ribbon { gradient, fill, .. } = s else { unreachable!() };
                assert_eq!(gradient.stops.len(), 3);
                assert_eq!(&gradient.stops[1], fill);
                // All three stops lie on the run of one same palette.
                let on_one = COMET.iter().any(|row| gradient.stops.iter().all(|h| off_run(h, row) <= 1.5));
                assert!(on_one, "{color}: {:?} is not on a comet palette", gradient.stops);
            }
            // Where one piece of a ribbon meets the next, the colour carries straight on.
            let num = |s: &FrameShape| match s {
                FrameShape::Ribbon { key, .. } => key.split('-').nth(1).unwrap().parse::<usize>().unwrap(),
                _ => unreachable!(),
            };
            let mut ordered = shapes.clone();
            ordered.sort_by_key(num);
            for k in 1..ordered.len() {
                let (FrameShape::Ribbon { gradient: p, .. }, FrameShape::Ribbon { gradient: n, .. }) =
                    (&ordered[k - 1], &ordered[k])
                else {
                    unreachable!()
                };
                if (p.x2 - n.x1).hypot(p.y2 - n.y1) < 1e-6 {
                    assert_eq!(n.stops[0], p.stops[2]);
                }
            }
        }
    }
    // A celebration's nine ribbons use several of the six palettes.
    let f = ribbons_at(colors[0], 5);
    let rows: std::collections::BTreeSet<usize> = f
        .iter()
        .filter_map(|s| match s {
            FrameShape::Ribbon { gradient, .. } => {
                COMET.iter().position(|row| gradient.stops.iter().all(|h| off_run(h, row) <= 1.5))
            }
            _ => None,
        })
        .collect();
    assert!(rows.len() > 1, "{rows:?}");
}

/// MausBot: the smile. It is an extra eye-keyed shape ("mouth", a stroked arc) shown with both eyes.
#[test]
fn mausbot_draws_a_smile_between_the_eyes_while_both_eyes_are_visible() {
    let mouth = |o: &RenderOptions| {
        frame(&mug(), o)
            .shapes
            .into_iter()
            .filter_map(|s| match s {
                FrameShape::Eye { key: "mouth", mark, .. } => Some(mark),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let facing = mouth(&still());
    assert_eq!(facing.len(), 1);
    assert!(is_arc(&facing[0]));
    // Turned right round the front is hidden, and so is the smile.
    assert!(mouth(&RenderOptions { view: view(PI, 0.0), ..still() }).is_empty());
}

/// states.test.ts "stay where they are while the body hops and rolls": inside the frame's root
/// motion a ribbon's matrix undoes it, so on screen it has not moved.
#[test]
fn ribbons_stay_where_they_are_while_the_body_hops_and_rolls() {
    let mut n = 0;
    // A working turn is out only some of the time: look through a few moments until one is.
    for k in 0..40 {
        let t = 1.2 + k as f64 * 0.05;
        let f2 = frame(
            &mug(),
            &RenderOptions { state: Some(AvatarState::Working), time: Some(t), state_age: Some(t), ..opts() },
        );
        let [a, b, c, d, e, g] = f2.root;
        for r in ribbons(&f2) {
            let m = matrix_of(r);
            let on = [
                a * m[0] + c * m[1],
                b * m[0] + d * m[1],
                a * m[2] + c * m[3],
                b * m[2] + d * m[3],
                a * m[4] + c * m[5] + e,
                b * m[4] + d * m[5] + g,
            ];
            for (v, want) in on.iter().zip([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]) {
                near(*v, want, 1e-6);
            }
            n += 1;
        }
    }
    assert!(n > 0, "no ribbon was out to check");
}

/// states.test.ts "stay within the reach the canvas allows for, on every avatar".
#[test]
fn ribbons_stay_within_the_reach_the_canvas_allows_for_on_every_avatar() {
    for d in everyone() {
        let (prepared, vol) = prepared_volume(&d, None).unwrap();
        let b = vol.bounds;
        let rims = ribbon_rims(&analyze_body(&prepared.body_path).unwrap().polys, &b);
        let extent = avatar_extent(&d, None).unwrap();
        for i in 0..40 {
            let rim = if i % 2 == 1 { &rims.1 } else { &rims.0 };
            let reach = ribbon_reach(Some(rim.max));
            for piece in
                ribbon_pieces(&b, 1.0, hash_string(&d.id), i as f64 * 0.21, i as f64 * 0.6, (i % 2) as f64, Some(rim))
            {
                let bx = piece.bbox;
                assert!(bx.x >= extent.x && bx.y >= extent.y, "{}", d.id);
                assert!(bx.x + bx.w <= extent.x + extent.size && bx.y + bx.h <= extent.y + extent.size, "{}", d.id);
                assert!(bx.w.max(bx.h) <= 2.0 * (reach.radius + reach.band), "{}", d.id);
            }
        }
    }
}

/// states.test.ts "ride hoops that hug each avatar's outline, and pass behind it only outside it".
#[test]
fn ribbons_ride_hoops_that_hug_each_outline_and_pass_behind_it_only_outside_it() {
    let all = everyone();
    assert!(all.len() > 20);
    for d in &all {
        let (prepared, vol) = prepared_volume(d, None).unwrap();
        let b = vol.bounds;
        let polys = analyze_body(&prepared.body_path).unwrap().polys;
        let seed = hash_string(&d.id);
        let rims = ribbon_rims(&polys, &b);
        let mut nearest = f64::INFINITY;
        let mut far = [0.0_f64; 2];
        for (state, from, to) in [(AvatarState::Done, 0.3, DONE_TURN_S), (AvatarState::Working, 0.0, 9.0)] {
            let mut t = from;
            while t <= to {
                let pose = compute_pose(&PoseInput {
                    seed,
                    time: t,
                    state: Some(state),
                    state_age: Some(t),
                    ..PoseInput::default()
                });
                let which = usize::from(pose.cage >= 0.5);
                let rim = ribbons_follow(if which == 1 { &rims.1 } else { &rims.0 }, &pose, &b);
                let pieces = ribbon_pieces(&b, pose.ribbons, seed, t, pose.sweep, pose.cage, Some(&rim));
                // The body as it is posed this frame (leaning, lifted, stretched): the ribbons stay put.
                let [m0, m1, m2, m3, m4, m5] =
                    frame(d, &RenderOptions { state: Some(state), time: Some(t), state_age: Some(t), ..opts() }).root;
                let posed: Vec<Vec<Point>> = polys
                    .iter()
                    .map(|poly| poly.iter().map(|[x, y]| [m0 * x + m2 * y + m4, m1 * x + m3 * y + m5]).collect())
                    .collect();
                for k in 1..pieces.len() {
                    let (p, n) = (&pieces[k - 1], &pieces[k]);
                    if p.ribbon != n.ribbon || p.front == n.front {
                        continue;
                    }
                    // Where a ribbon passes from one layer to the other, its centre is clear of the body...
                    let [x, y] = n.from;
                    assert!(!is_inside(&posed, x, y), "{} {state:?} t={t}", d.id);
                    nearest = nearest.min(edge_distance(&posed, x, y));
                }
                for q in &pieces {
                    far[which] = far[which].max((q.to[0] - rim.cx).hypot(q.to[1] - rim.cy));
                }
                t += 0.05;
            }
        }
        // ...but only just: the innermost ribbons pass close by the outline, on every avatar.
        if nearest.is_finite() {
            assert!(nearest < 10.0, "{}: {nearest}", d.id);
        }
        for (which, rim) in [&rims.0, &rims.1].into_iter().enumerate() {
            let r = ribbon_reach(Some(rim.max));
            assert!(far[which] <= r.radius + r.band, "{}", d.id);
        }
    }
}

/// states.test.ts "move smoothly from frame to frame, drawing in included: no stroke jumps".
#[test]
fn ribbons_move_smoothly_from_frame_to_frame() {
    let m = mug();
    let b = prepared_volume(&m, None).unwrap().1.bounds;
    let seed = hash_string(&m.id);
    let radius = ribbon_reach(None).radius;
    for (state, from, to) in [(AvatarState::Done, 0.3, DONE_TURN_S), (AvatarState::Working, 0.0, 12.0)] {
        let mut last: std::collections::HashMap<usize, [f64; 2]> = Default::default();
        let (mut sweep, mut worst) = (0.0_f64, 0.0_f64);
        let mut i = 0;
        loop {
            let t = from + i as f64 / 60.0;
            if t > to {
                break;
            }
            i += 1;
            let pose = compute_pose(&PoseInput {
                seed,
                time: t,
                state: Some(state),
                state_age: Some(t),
                ..PoseInput::default()
            });
            let mut heads = std::collections::HashMap::new();
            for p in ribbon_pieces(&b, pose.ribbons, seed, t, pose.sweep, pose.cage, None) {
                if p.along[1] == 1.0 {
                    heads.insert(p.ribbon, p.to);
                }
            }
            // At most as far as the turn carried it this frame round the widest hoop, plus its own drift.
            let allowed = (pose.sweep - sweep).abs() * radius + 1.5;
            for (id, [x, y]) in &heads {
                if let Some(before) = last.get(id) {
                    worst = worst.max((x - before[0]).hypot(y - before[1]) / allowed);
                }
            }
            last = heads;
            sweep = pose.sweep;
        }
        assert!(worst < 1.0, "{state:?}: {worst}");
    }
}

/// states.test.ts "draw out and back in with their presence".
#[test]
fn ribbons_draw_out_and_back_in_with_their_presence() {
    let b = prepared_volume(&mug(), None).unwrap().1.bounds;
    assert!(ribbon_pieces(&b, 0.0, 3, 1.0, 0.0, 0.0, None).is_empty());
    // Placed by how far a turn has carried them: with no sweep there is nothing to draw yet.
    assert!(ribbon_pieces(&b, 1.0, 3, 1.0, 0.0, 0.0, None).is_empty());
    let area = |k: f64, sweep: f64, cage: f64| {
        ribbon_pieces(&b, k, 3, 1.0, sweep, cage, None).iter().map(|p| p.bbox.w * p.bbox.h).sum::<f64>()
    };
    assert!(area(0.3, 4.0, 0.0) < area(1.0, 4.0, 0.0));
    let lanes = |cage: f64| {
        let mut v: Vec<usize> = ribbon_pieces(&b, 1.0, 3, 1.0, 12.0, cage, None).iter().map(|p| p.ribbon).collect();
        v.sort();
        v.dedup();
        v
    };
    // A working turn: three to five ribbons on one hoop.
    let working = lanes(0.0);
    assert!(working.len() >= 3 && working.len() <= 5, "{working:?}");
    assert_eq!(working, (0..working.len()).collect::<Vec<_>>());
    // A celebration has three ribbons on each of three crossing hoops.
    assert_eq!(lanes(1.0), (0..3 * LANES).collect::<Vec<_>>());
    // A second and a half into each: a celebration's turn has gone much further by then.
    assert!(area(1.0, 22.0, 1.0) > area(1.0, 6.0, 0.0));
    // Drawing in keeps the layout: fewer, shorter ribbons in the same places.
    let layout = |k: f64| {
        ribbon_pieces(&b, k, 3, 1.0, 30.0, 1.0, None)
            .iter()
            .map(|p| p.ribbon)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(layout(0.3), layout(1.0));
    let pieces =
        |sweep: f64| ribbon_pieces(&b, 1.0, 3, 1.0, sweep, 0.0, None).iter().map(|p| p.d.clone()).collect::<Vec<_>>();
    assert_ne!(pieces(2.5), pieces(2.0));
    // Turning the other way, they go round the other way.
    assert_ne!(pieces(-2.0), pieces(2.0));
}

fn done_input(state_age: Option<f64>) -> PoseInput {
    PoseInput { seed: 5, time: 4.0, state: Some(AvatarState::Done), state_age, ..PoseInput::default() }
}

fn resting() -> crate::avatar::oba::motion::Pose {
    compute_pose(&done_input(Some(DONE_TURN_S + 1.0)))
}

/// states.test.ts "celebrates as it begins: a few turns with more ribbons than a working bot has,
/// then a rest".
#[test]
fn done_celebrates_as_it_begins_a_few_turns_then_a_rest() {
    let rest = resting();
    assert_eq!(rest.ribbons, 0.0);
    assert_eq!(compute_pose(&done_input(Some(DONE_CYCLE_S - 0.01))), compute_pose(&done_input(Some(DONE_TURN_S))));
    // Without an age it is at rest.
    assert_eq!(compute_pose(&done_input(None)), rest);
    let mid = compute_pose(&done_input(Some(DONE_TURN_S / 2.0)));
    assert_eq!(mid.ribbons, 1.0);
    assert_eq!(mid.cage, 1.0);
    assert!(mid.lift < rest.lift);
    // The body faces every way in turn, and ends where it rests.
    let (mut turned, mut last) = (0.0, 0.0);
    let mut i = 0;
    loop {
        let u = i as f64 * 0.002;
        if u > 1.0001 {
            break;
        }
        i += 1;
        let yaw = compute_pose(&done_input(Some(u.min(0.999999) * DONE_TURN_S))).yaw - rest.yaw;
        turned += (yaw - last).sin().atan2((yaw - last).cos());
        last = yaw;
    }
    near(turned / TAU, 3.0, 0.05);
    // It winds up the other way first.
    assert!(compute_pose(&done_input(Some(0.15))).yaw < rest.yaw);
}

/// states.test.ts "celebrates again after a rest, the other way round, at a pace of its own".
#[test]
fn done_celebrates_again_after_a_rest_the_other_way_round() {
    let rest = resting();
    // When the second celebration starts (to the millisecond).
    let again = |seed: u32| {
        let mut i = 0;
        loop {
            let age = DONE_TURN_S + i as f64 * 0.001;
            if age >= 2.0 * DONE_CYCLE_S {
                return f64::NAN;
            }
            if compute_pose(&PoseInput { seed, ..done_input(Some(age)) }).ribbons > 0.0 {
                return age;
            }
            i += 1;
        }
    };
    let cycle = again(5);
    assert!((DONE_CYCLE_S..DONE_CYCLE_S * 1.16).contains(&cycle), "{cycle}");
    let first = compute_pose(&done_input(Some(1.0))).yaw - rest.yaw;
    let second = compute_pose(&done_input(Some(cycle + 1.0))).yaw - rest.yaw;
    // A millisecond of the turn is a hundredth of a radian at full speed.
    near(second, -first, 0.05);
    assert!((again(6) - cycle).abs() > 0.05);
}

/// states.test.ts "never unwinds: the body stays the short way round from rest, the ribbons go on
/// round with it".
#[test]
fn done_never_unwinds() {
    let rest = resting();
    let mut last = 0.0;
    for i in 1..100 {
        let u = i as f64 * 0.01;
        let p = compute_pose(&done_input(Some(u * DONE_TURN_S)));
        assert!((p.yaw - rest.yaw).abs() <= PI + 1e-9);
        // The sweep is not wrapped: past the wind-up it only grows, to three times the body's turns.
        if u > 0.2 {
            assert!(p.sweep >= last, "u = {u}");
        }
        last = p.sweep;
    }
    assert!(last > 8.0 * TAU);
    // Leaving the state mid-celebration, the ribbons fade where they are instead of winding back.
    let mid = compute_pose(&done_input(Some(DONE_TURN_S / 2.0)));
    let away =
        blend_poses(&mid, &compute_pose(&PoseInput { state: Some(AvatarState::Idle), ..done_input(Some(0.0)) }), 0.5);
    assert_eq!(away.sweep, mid.sweep);
    // Neither angle wraps while the state is still easing in.
    let mut age = 0.0;
    while age < MOOD_MS / 1000.0 {
        let p = compute_pose(&done_input(Some(age)));
        assert!((p.yaw - rest.yaw).abs() < 3.0 && p.sweep.abs() < 3.0, "age {age}");
        age += 0.01;
    }
}

/// states.test.ts "does not celebrate under reduced motion".
#[test]
fn done_does_not_celebrate_under_reduced_motion() {
    let calm = |age| compute_pose(&PoseInput { reduced_motion: true, ..done_input(age) });
    assert_eq!(calm(Some(1.0)), calm(None));
    assert_eq!(calm(Some(1.0)).ribbons, 0.0);
}
