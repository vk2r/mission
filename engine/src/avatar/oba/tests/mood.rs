//! Ported from upstream `mood.test.ts` ("mood changes"). The "animator" suite of that file tests
//! `animator.ts`, which is not part of this port.

use super::*;
use crate::avatar::oba::mood::{MOOD_MS, MoodState, mood_progress, settle_mood};
use crate::avatar::oba::motion::{PoseInput, blend_poses, compute_pose};
use crate::avatar::oba::types::{AvatarMotionBlend, BlendFrom};

fn base(motion: AvatarMotion) -> PoseInput {
    PoseInput { seed: 7, time: 3.2, reduced_motion: false, motion: Some(motion), ..PoseInput::default() }
}

/// mood.test.ts "blend from one pose to the other, exactly at both ends".
#[test]
fn mood_poses_blend_exactly_at_both_ends() {
    let a = compute_pose(&base(AvatarMotion::Idle));
    let b = compute_pose(&base(AvatarMotion::Thinking));
    assert_eq!(blend_poses(&a, &b, 0.0), a);
    assert_eq!(blend_poses(&a, &b, 1.0), b);
    let mid = blend_poses(&a, &b, 0.5);
    assert!((mid.roll - (a.roll + b.roll) / 2.0).abs() < 1e-9);
}

/// mood.test.ts "morph the eyes from one shape to the other, with no blink to hide a swap".
#[test]
fn mood_eyes_morph_with_no_blink_to_hide_a_swap() {
    let a = compute_pose(&base(AvatarMotion::Idle));
    let b = compute_pose(&base(AvatarMotion::Happy));
    assert_eq!(a.eyes.arc, 0.0);
    assert_eq!(b.eyes.arc, 1.0);
    assert!((blend_poses(&a, &b, 0.25).eyes.arc - 0.25).abs() < 1e-9);
    assert!((blend_poses(&a, &b, 0.5).blink - (a.blink + b.blink) / 2.0).abs() < 1e-9);
}

/// mood.test.ts "ease in through computePose".
#[test]
fn mood_eases_in_through_compute_pose() {
    let from = compute_pose(&base(AvatarMotion::Idle));
    let to = compute_pose(&base(AvatarMotion::Happy));
    let with = |t: f64| PoseInput {
        blend: Some(AvatarMotionBlend { from: BlendFrom::Motion(AvatarMotion::Idle), t }),
        ..base(AvatarMotion::Happy)
    };
    assert_eq!(compute_pose(&with(0.0)), from);
    assert_eq!(compute_pose(&with(1.0)), to);
}

/// mood.test.ts "progress eases from 0 to 1 over MOOD_MS".
#[test]
fn mood_progress_eases_from_0_to_1_over_mood_ms() {
    assert_eq!(mood_progress(0.0), 0.0);
    assert_eq!(mood_progress(MOOD_MS), 1.0);
    assert!((mood_progress(MOOD_MS / 2.0) - 0.5).abs() < 1e-9);
}

/// mood.test.ts "never grows the chain of interrupted changes".
#[test]
fn mood_never_grows_the_chain_of_interrupted_changes() {
    let mut state: MoodState = MoodState { state: Some(AvatarState::Idle), ..MoodState::default() };
    let states = [AvatarState::Thinking, AvatarState::Working, AvatarState::Idle];
    for i in 0..50 {
        let from = settle_mood(&state, 1.0, 0);
        state = MoodState {
            state: Some(states[i % 3]),
            mood: Some(AvatarMood::Happy),
            age: Some(i as f64),
            blend: Some(Box::new(AvatarMotionBlend { from: BlendFrom::Look(from), t: 0.3 })),
            ..MoodState::default()
        };
    }
    let mut depth = 0;
    let mut cur = &state;
    while let Some(b) = &cur.blend {
        depth += 1;
        match &b.from {
            BlendFrom::Look(l) => cur = l,
            BlendFrom::Motion(_) => break,
        }
    }
    assert!(depth <= 5, "chain depth {depth}");
}
