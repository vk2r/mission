//! Avatar designs from Open Bot Avatar by Dominik Scholz.
//!
//! This module is a Rust port of the engine core of Open Bot Avatar (`packages/core`, MIT):
//! the avatar contract, the implied-volume renderer, the pose and eye model, the ribbons and
//! marks, and the starter avatars. Ported from commit 5e64c08 (npm `@open-bot-avatar/core`
//! 0.3.0). Upstream is now at 0.4.0, which removes the Fluffy avatar. Left out: `dom/`, the
//! animator, `follow` and `seams`. The two MausBot differences from upstream are marked with
//! "MausBot:" comments in `render.rs` (the mouth and the comet ribbon palettes).
//!
//! The licence text is kept in `LICENSE` next to this file; `scripts/licenses.sh`
//! (`mise run licenses`) copies it into `licenses/THIRD-PARTY.txt`.
//!
//! Nothing outside `avatar::oba` is referenced from here, and nothing is wired to the stream,
//! roster or publish code yet.

// The port keeps the whole upstream API; parts only the tests use are not called yet.
#![allow(dead_code)]

pub mod catalog;
pub mod color;
pub mod contour;
pub mod effects;
pub mod examples;
pub mod eyes;
pub mod mood;
pub mod motion;
pub mod path;
pub mod render;
#[cfg(test)]
mod tests;
pub mod types;
pub mod validate;
pub mod volume;
