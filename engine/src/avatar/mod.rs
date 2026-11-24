pub mod looks;
pub mod mausbot;
pub mod oba;
pub mod publish;
pub mod skin;

use mausbot::festival;
use mausbot::mascot::{self, Look, Part};
use oba::catalog;
use oba::path::{bounds, flatten_path, fmt, parse_path};
use oba::render::{avatar_extent, render_avatar_svg};
use oba::motion::{BASE_PITCH, BASE_YAW, LOOK_PITCH, LOOK_YAW};
use oba::types::{AvatarDefinition, Gaze, RenderOptions, View};

/// The size, in pixels, every avatar picture is drawn at: the slot the bar shows it in.
const PICTURE_SIZE: f64 = 96.0;

/// Who draws a body, decided as `MausAvatar` does (`Avatar.tsx:116-152`).
#[derive(Debug, Clone, Copy)]
pub enum Painter {
    /// A carved festival costume (`isCarvedBody`, `Avatar.tsx:117`): `mausbot::festival`.
    Festival,
    /// One of the nine shapes or the 21 objects (`engineBody`, `Avatar.tsx:136`): `oba`.
    Oba(&'static AvatarDefinition),
    /// Everything else, the cursor included (`GradientAvatar`, `Avatar.tsx:152`): `mausbot::mascot`.
    Cursor,
}

/// Carved first, then the engine's bodies, then the cursor.
pub fn painter(body: &str) -> Painter {
    if festival::is_body(body) {
        return Painter::Festival;
    }
    match catalog::engine_body(body) {
        Some(definition) => Painter::Oba(definition),
        None => Painter::Cursor,
    }
}

/// The picture of one part of a bot's look, from whoever paints its body.
pub fn avatar_svg(look: &Look, part: Part) -> Option<String> {
    match painter(&look.body) {
        Painter::Festival => festival::avatar_svg(look, part),
        // The engine draws the eyes inside the picture, so there is no second one to lay over it.
        Painter::Oba(definition) => (part == Part::Body).then(|| oba_svg(definition, look)).flatten(),
        Painter::Cursor => mascot::avatar_svg(look, part),
    }
}

/// Where the eyes turn from, as a fraction of the picture. Only the carved and cursor eyes are a
/// separate picture that can turn.
pub fn eye_pivot(look: &Look) -> Option<[f64; 2]> {
    match painter(&look.body) {
        Painter::Festival => festival::eye_pivot(look),
        Painter::Oba(_) => None,
        Painter::Cursor => mascot::eye_pivot(look),
    }
}

/// How many pictures a face can take in turn; none for the bodies the engine moves itself.
pub fn variant_count(look: &Look) -> usize {
    match painter(&look.body) {
        Painter::Cursor => mascot::variant_count(&look.expression),
        Painter::Festival | Painter::Oba(_) => 0,
    }
}

/// The options an engine body is drawn with: filled and inked as `Avatar.tsx:141` and
/// `ObjectAvatar.tsx:75` do, in the state `looks` gives its face.
fn oba_options(look: &Look, shown: looks::Shown) -> RenderOptions {
    RenderOptions {
        color: Some(catalog::body_fill(look.paint.body_color.as_deref(), &look.paint.colour_name)),
        eye_color: catalog::face_ink(look.paint.face_color.as_deref()).map(str::to_string),
        size: Some(PICTURE_SIZE),
        state: Some(shown.state),
        mood: Some(shown.mood),
        ..RenderOptions::default()
    }
}

/// An engine body at rest, as the bar shows it when nothing moves it: drawn `still`. The viewBox is the
/// avatar's own extent, not 0 0 100 100, because Qt ignores `overflow="visible"` and would cut the
/// ribbons off at the box; `extent` says how to lay it over the slot.
fn oba_svg(definition: &AvatarDefinition, look: &Look) -> Option<String> {
    let shown = looks::shown(&look.expression, look.paint.cheering, Some(look.paint.working));
    let options = RenderOptions { time: Some(0.0), reduced_motion: true, ..oba_options(look, shown) };
    in_extent(definition, render_avatar_svg(definition, &options).ok()?)
}

/// One frame of a moving engine body, `age_s` seconds into its state, or `None` for a body that is at
/// rest (`looks::shown` says whether it moves) and looks nowhere, or whose drawing fails. `gaze` is where
/// it looks, each axis -1..1: a body that is at rest then gets a calm frame with its eyes turned, the
/// still picture `oba_svg` draws but for the gaze.
pub fn oba_frame_svg(definition: &AvatarDefinition, look: &Look, age_s: f64, gaze: Option<[f64; 2]>) -> Option<String> {
    let shown = looks::shown(&look.expression, look.paint.cheering, Some(look.paint.working));
    // How much it attends: nothing at the centre, all of it from a fifth of the range, so a gaze that
    // eases back to the centre also eases the body back to its resting turn instead of snapping.
    let attention = gaze.map(|[x, y]| (x.hypot(y) / 0.2).min(1.0));
    let options = match (shown.live, gaze, attention) {
        (true, Some([x, y]), Some(weight)) => RenderOptions {
            time: Some(age_s),
            state_age: Some(age_s),
            gaze: Some(Gaze { x, y, weight: Some(weight) }),
            ..oba_options(look, shown)
        },
        (true, ..) => RenderOptions { time: Some(age_s), state_age: Some(age_s), ..oba_options(look, shown) },
        (false, Some([x, y]), Some(weight)) => {
            // Reduced motion keeps the body still, so the turn the app gives a body that looks somewhere
            // (`motion.rs`, LOOK_YAW and LOOK_PITCH) is added here as a view rotation.
            let view = View {
                yaw: weight * (x * LOOK_YAW - BASE_YAW),
                pitch: weight * (-y * LOOK_PITCH - BASE_PITCH / 2.0),
            };
            RenderOptions {
                time: Some(0.0),
                reduced_motion: true,
                gaze: Some(Gaze { x, y, weight: None }),
                view: Some(view),
                ..oba_options(look, shown)
            }
        }
        (false, ..) => return None,
    };
    in_extent(definition, render_avatar_svg(definition, &options).ok()?)
}

fn in_extent(definition: &AvatarDefinition, svg: String) -> Option<String> {
    let extent = avatar_extent(definition, None).ok()?;
    let view_box = format!("viewBox=\"{} {} {} {}\"", fmt(extent.x), fmt(extent.y), fmt(extent.size), fmt(extent.size));
    Some(svg.replacen("viewBox=\"0 0 100 100\"", &view_box, 1))
}

/// Where an engine body's 0..100 box sits in its picture: `[x, y, size]` in viewBox units. The bar
/// draws the picture `size / 100` as big, so the body fills its slot as in the app and the ribbons
/// spill past it.
pub fn extent(look: &Look) -> Option<[f64; 3]> {
    let Painter::Oba(definition) = painter(&look.body) else {
        return None;
    };
    let extent = avatar_extent(definition, None).ok()?;
    Some([extent.x, extent.y, extent.size].map(round_hundredths))
}

/// How far the body's ink starts from the left of its slot, in 22nds of the slot: the bar stops its
/// connector line there. The cursor's is 3 and the costumes' 0, as `Panel.qml` had them; an engine
/// body's is where its path starts in the 0..100 box.
pub fn ink_inset(look: &Look) -> f64 {
    match painter(&look.body) {
        Painter::Festival => 0.0,
        Painter::Cursor => CURSOR_INK_INSET,
        Painter::Oba(definition) => parse_path(&definition.body_path)
            .map(|segments| round_hundredths(bounds(&flatten_path(&segments)).min_x / 100.0 * SLOT_UNITS))
            .unwrap_or(0.0),
    }
}

fn round_hundredths(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// The bar's slot for an avatar is `Style.space(22)` wide: the unit `ink_inset` counts in.
const SLOT_UNITS: f64 = 22.0;

/// The cursor's ink inset in those units (`cursor: 3` of `Panel.qml`'s old `mascotInk` table).
const CURSOR_INK_INSET: f64 = 3.0;

#[cfg(test)]
mod tests;
