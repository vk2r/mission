//! Changes of state and mood as pure functions a frame loop steps (ported from `mood.ts`): the
//! pose eases from what is on screen into the new state and mood (see `blend_poses` in
//! `motion.rs`), and a change during a change continues from where it is.

use super::types::{AvatarMotionBlend, BlendFrom, BlendLook};

/// Length of a mood change, ms.
pub const MOOD_MS: f64 = 420.0;

fn smooth(t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Eased progress of a mood change from elapsed time (smoothstep, clamped).
pub fn mood_progress(elapsed_ms: f64) -> f64 {
    smooth(elapsed_ms / MOOD_MS)
}

/// A state and mood on screen, possibly still easing in over what came before.
pub type MoodState = BlendLook;

/// What a new mood change eases in from: the mood on screen with any change
/// still under way frozen at its progress, so the pose continues from exactly
/// where it is. Older changes whose share has all but faded (or beyond a few
/// levels) are dropped, so rapid clicking never grows the chain.
pub fn settle_mood(state: &MoodState, share: f64, depth: u32) -> MoodState {
    let Some(b) = &state.blend else { return state.clone() };
    let mut settled = state.clone();
    settled.blend = None;
    let rest = share * (1.0 - b.t);
    if rest < 0.02 || depth >= 3 {
        return settled;
    }
    let from = match &b.from {
        BlendFrom::Motion(m) => BlendFrom::Motion(*m),
        BlendFrom::Look(l) => BlendFrom::Look(settle_mood(l, rest, depth + 1)),
    };
    settled.blend = Some(Box::new(AvatarMotionBlend { from, t: b.t }));
    settled
}
