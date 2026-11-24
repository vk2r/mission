//! Avatar definition contract v1 (ported from `types.ts`).
//!
//! A definition is geometry only: one monochrome silhouette path in a fixed
//! 0 0 100 100 viewBox plus a placement for the eye pair. Everything else
//! (color, implied volume, view, gaze, blinking, motion) is owned by the
//! runtime renderer.

/// Placement of the eye pair, viewBox units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AvatarEyes {
    /// Center of the eye pair.
    pub x: f64,
    pub y: f64,
    /// Center-to-center distance between the two eyes.
    pub spacing: f64,
    /// 1 = each neutral eye is ~4 units wide and ~11 units tall.
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AvatarDefinition {
    pub version: u32,
    /// Lowercase slug, e.g. "controller".
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// One filled path, nonzero fill rule; holes wind opposite to their outline.
    pub body_path: String,
    pub eyes: AvatarEyes,
}

/// What the bot is doing. `Thinking` and `Blocked` replace the body with a mark (three dots, an
/// exclamation mark); `Working` sweeps a band of ribbons round the body every few seconds; `Done`
/// celebrates: it turns a few times with ribbons flying, rests, and after a while does it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvatarState {
    Idle,
    Thinking,
    Working,
    Waiting,
    Blocked,
    Done,
}

/// How the bot feels. It shapes the eyes on top of any state; in `Idle` a happy bot also hops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvatarMood {
    Neutral,
    Happy,
}

/// The single option that came before `state` and `mood`, still accepted: `Idle` and `Thinking`
/// are states, `Happy` is the idle state in a happy mood.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvatarMotion {
    Idle,
    Thinking,
    Happy,
}

/// A state/mood snapshot that a blend eases away from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlendLook {
    pub state: Option<AvatarState>,
    pub mood: Option<AvatarMood>,
    pub motion: Option<AvatarMotion>,
    /// Seconds that state had been showing when the change began (see `RenderOptions::state_age`).
    pub age: Option<f64>,
    pub blend: Option<Box<AvatarMotionBlend>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlendFrom {
    Motion(AvatarMotion),
    Look(BlendLook),
}

/// A change easing in over what was shown before it: a plain legacy motion, or a state and mood
/// that were themselves still easing in (their progress frozen where it was). `t` 0..1.
#[derive(Debug, Clone, PartialEq)]
pub struct AvatarMotionBlend {
    pub from: BlendFrom,
    pub t: f64,
}

/// Look target, each axis normalized -1..1 (x right, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gaze {
    pub x: f64,
    pub y: f64,
    /// 0..1, default 1.
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub yaw: f64,
    pub pitch: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderOptions {
    /// Body color as #rgb or #rrggbb. Invalid values fall back to the default.
    pub color: Option<String>,
    /// Eye color as #rgb or #rrggbb. Defaults to a plain near-black ink.
    pub eye_color: Option<String>,
    /// Rendered width/height in CSS pixels. Omit for a fluid SVG.
    pub size: Option<f64>,
    /// Volume amount 0..14 (default 6 = natural depth; 0 = flat).
    pub depth: Option<f64>,
    /// Extra view rotation in radians on top of the resting view.
    pub view: Option<View>,
    /// Animation clock in seconds. Same inputs always produce the same SVG.
    pub time: Option<f64>,
    /// Look target; overrides idle eye wander and turns the body toward it.
    pub gaze: Option<Gaze>,
    /// What the bot is doing (default `Idle`).
    pub state: Option<AvatarState>,
    /// How the bot feels (default `Neutral`).
    pub mood: Option<AvatarMood>,
    /// Legacy; `state` and `mood` win when given.
    pub motion: Option<AvatarMotion>,
    /// Seconds since `state` began.
    pub state_age: Option<f64>,
    /// Ease into `state` and `mood` from what was shown before.
    pub motion_blend: Option<AvatarMotionBlend>,
    /// Freeze body motion, blinking, ribbons and marks at a calm pose; gaze still applies.
    pub reduced_motion: bool,
}
