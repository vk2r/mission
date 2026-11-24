//! Tests ported from the non-DOM upstream suites in Open Bot Avatar's `packages/core/tests`
//! (frame, mood, paint, gallery, avatar, states, volume). Each test names the original test it
//! comes from. Tests of `dom/`, the animator, the CSS scene and `seams.ts` are not ported, since
//! those modules are not part of this port. Where MausBot's render differs from upstream (the
//! mouth, the comet ribbon palettes), the test says how it was adapted.

mod avatar;
mod frame;
mod gallery;
mod mood;
mod paint;
mod sample;
mod states;
mod volume;

use super::examples::{examples, get_example};
use super::path::Point;
use super::render::{AvatarFrame, EyeMark, FrameShape};
use super::types::{AvatarDefinition, AvatarMood, AvatarMotion, AvatarState, Gaze, RenderOptions, View};
use super::validate::validate_avatar;

/// The sixteen gallery avatars' JSON files (everything in upstream `gallery/` but `colors.json`),
/// copied next to the tests so they do not depend on files outside this repository.
const GALLERY: [&str; 16] = [
    include_str!("gallery/book.json"),
    include_str!("gallery/brain.json"),
    include_str!("gallery/bull.json"),
    include_str!("gallery/bunny.json"),
    include_str!("gallery/camera.json"),
    include_str!("gallery/cat.json"),
    include_str!("gallery/cloud.json"),
    include_str!("gallery/egg.json"),
    include_str!("gallery/ghost.json"),
    include_str!("gallery/magnifier.json"),
    include_str!("gallery/paint-brush.json"),
    include_str!("gallery/palette.json"),
    include_str!("gallery/rocket.json"),
    include_str!("gallery/shield.json"),
    include_str!("gallery/star.json"),
    include_str!("gallery/witch-hat.json"),
];

/// Gallery files with their names, parsed and validated.
fn gallery() -> Vec<(String, AvatarDefinition)> {
    GALLERY
        .iter()
        .map(|raw| {
            let value: serde_json::Value = serde_json::from_str(raw).expect("gallery file is JSON");
            let def = validate_avatar(&value).unwrap_or_else(|e| panic!("gallery avatar invalid: {e:?}"));
            (format!("{}.json", def.id), def)
        })
        .collect()
}

/// The starters and every avatar in the gallery (upstream `everyone` in states.test.ts).
fn everyone() -> Vec<AvatarDefinition> {
    let mut all = examples();
    all.extend(gallery().into_iter().map(|(_, d)| d));
    all
}

fn ex(id: &str) -> AvatarDefinition {
    get_example(id).unwrap_or_else(|| panic!("no example {id}"))
}

fn opts() -> RenderOptions {
    RenderOptions::default()
}

fn at(time: f64) -> RenderOptions {
    RenderOptions { time: Some(time), ..opts() }
}

fn state_at(state: AvatarState, time: f64) -> RenderOptions {
    RenderOptions { state: Some(state), time: Some(time), ..opts() }
}

fn still() -> RenderOptions {
    RenderOptions { reduced_motion: true, ..opts() }
}

fn view(yaw: f64, pitch: f64) -> Option<View> {
    Some(View { yaw, pitch })
}

fn gaze(x: f64, y: f64) -> Option<Gaze> {
    Some(Gaze { x, y, weight: None })
}

fn motion(m: AvatarMotion, time: f64) -> RenderOptions {
    RenderOptions { motion: Some(m), time: Some(time), ..opts() }
}

fn happy_at(time: f64) -> RenderOptions {
    RenderOptions { mood: Some(AvatarMood::Happy), time: Some(time), ..opts() }
}

fn svg(d: &AvatarDefinition, o: &RenderOptions) -> String {
    super::render::render_avatar_svg(d, o).expect("renders")
}

fn frame(d: &AvatarDefinition, o: &RenderOptions) -> AvatarFrame {
    super::render::avatar_frame(d, o).expect("frames")
}

/// The eye marks of a frame in painting order. MausBot: the smile is an `Eye` shape too, with key
/// "mouth"; `eyes_of` leaves it out so the counts are upstream's.
fn eyes_of(f: &AvatarFrame) -> Vec<&EyeMark> {
    f.shapes
        .iter()
        .filter_map(|s| match s {
            FrameShape::Eye { key, mark, .. } if *key != "mouth" => Some(mark),
            _ => None,
        })
        .collect()
}

fn is_mark(s: &FrameShape) -> bool {
    matches!(s, FrameShape::Slice { key, .. } if key.starts_with("mark-"))
}

fn is_body(s: &FrameShape) -> bool {
    matches!(s, FrameShape::Slice { key, .. } if !key.starts_with("mark-"))
}

fn ribbons_of(f: &AvatarFrame) -> Vec<&FrameShape> {
    f.shapes.iter().filter(|s| matches!(s, FrameShape::Ribbon { .. })).collect()
}

fn matrix_of(s: &FrameShape) -> [f64; 6] {
    match s {
        FrameShape::Slice { matrix, .. } | FrameShape::Eye { matrix, .. } | FrameShape::Ribbon { matrix, .. } => {
            *matrix
        }
    }
}

/// Every number in a string, as `str.match(/-?\d+(?:\.\d+)?/g).map(Number)`.
fn numbers(d: &str) -> Vec<f64> {
    let b = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        if b[i] == b'-' {
            i += 1;
        }
        let digits = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == digits {
            i = start + 1;
            continue;
        }
        if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        out.push(d[start..i].parse::<f64>().expect("number"));
    }
    out
}

fn flat_points(polys: &[Vec<Point>]) -> Vec<Point> {
    polys.iter().flatten().copied().collect()
}

fn to_json(d: &AvatarDefinition) -> serde_json::Value {
    serde_json::json!({
        "version": d.version,
        "id": d.id,
        "name": d.name,
        "body": { "path": d.body_path },
        "eyes": { "x": d.eyes.x, "y": d.eyes.y, "spacing": d.eyes.spacing, "scale": d.eyes.scale },
    })
}

/// The validation errors of a JSON input (none when it is valid), as avatar.test.ts `errorsOf`.
fn errors_of(input: &serde_json::Value) -> Vec<String> {
    validate_avatar(input).err().unwrap_or_default()
}
