//! Checks of the catalog against the app's tables. `app_tables.json` holds the thirty bodies as
//! `OBJECT_BODIES` and `SHAPE_BODIES` define them in the app's
//! `packages/phone-protocol/src/object-bodies.ts` and `open-bot-avatar/examples.ts`, read out of
//! that text (ids, names, paths, eyes) in the order the pickers show them. The gallery files of
//! Open Bot Avatar 0.3.0 (`tests/gallery/*.json`) are the second source for the sixteen gallery
//! avatars.

use super::super::examples::examples;
use super::super::render::{avatar_frame, render_avatar_svg};
use super::super::types::{AvatarDefinition, RenderOptions};
use super::super::validate::validate_avatar;
use super::objects::{OBJECT_BODY_IDS, object_bodies};
use super::shapes::shape_bodies;
use super::*;

const APP: &str = include_str!("app_tables.json");

/// One body as the app's tables list it.
struct Row {
    id: String,
    name: String,
    path: String,
    eyes: [f64; 4],
}

fn rows(key: &str) -> Vec<Row> {
    let app: serde_json::Value = serde_json::from_str(APP).expect("app_tables.json is JSON");
    app[key]
        .as_array()
        .unwrap_or_else(|| panic!("no {key} table"))
        .iter()
        .map(|r| {
            let eyes = r["eyes"].as_array().expect("eyes");
            Row {
                id: r["id"].as_str().expect("id").into(),
                name: r["name"].as_str().expect("name").into(),
                path: r["path"].as_str().expect("path").into(),
                eyes: std::array::from_fn(|i| eyes[i].as_f64().expect("number")),
            }
        })
        .collect()
}

fn eyes_of(d: &AvatarDefinition) -> [f64; 4] {
    [d.eyes.x, d.eyes.y, d.eyes.spacing, d.eyes.scale]
}

fn same(d: &AvatarDefinition, row: &Row) {
    assert_eq!(d.id, row.id);
    assert_eq!(d.name, row.name, "{} name", row.id);
    assert_eq!(d.body_path, row.path, "{} path", row.id);
    assert_eq!(eyes_of(d), row.eyes, "{} eyes", row.id);
    assert_eq!(d.version, 1);
}

/// Every one of the thirty ids matches the app's table on name, path and eyes, in picker order.
#[test]
fn the_thirty_bodies_equal_the_apps_tables() {
    let (objects, shapes) = (rows("objects"), rows("shapes"));
    assert_eq!((objects.len(), shapes.len()), (21, 9));
    let (ours_o, ours_s) = (object_bodies(), shape_bodies());
    assert_eq!(ours_o.len(), 21);
    assert_eq!(ours_s.len(), 9);
    for (d, row) in ours_o.iter().zip(&objects).chain(ours_s.iter().zip(&shapes)) {
        same(d, row);
        let found = engine_body(&row.id).unwrap_or_else(|| panic!("engine_body({}) is None", row.id));
        same(found, row);
    }
    let ids: Vec<&str> = ours_o.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, OBJECT_BODY_IDS);
    let ids: Vec<&str> = ours_s.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, SHAPE_BODY_IDS);
}

/// The ids are unique across both tables, objects carry "obj-" and shapes never do.
#[test]
fn ids_are_unique_and_objects_carry_the_prefix() {
    let all: Vec<String> = object_bodies().into_iter().chain(shape_bodies()).map(|d| d.id).collect();
    let mut sorted = all.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 30);
    assert!(object_bodies().iter().all(|d| d.id.starts_with("obj-")));
    assert!(shape_bodies().iter().all(|d| !d.id.starts_with("obj-")));
}

/// The starters in the objects are `examples()` and the other fourteen are the gallery files,
/// which also give Shield and Star their shapes (the app wears them under the ids "shield" and
/// "star"). Paths and eyes are compared with the JSON as written, before `validate_avatar`
/// rewrites `V`/`H` into `L`.
#[test]
fn objects_and_shapes_match_the_gallery_files() {
    let files: [(&str, &str); 16] = [
        ("book", include_str!("../tests/gallery/book.json")),
        ("brain", include_str!("../tests/gallery/brain.json")),
        ("bull", include_str!("../tests/gallery/bull.json")),
        ("bunny", include_str!("../tests/gallery/bunny.json")),
        ("camera", include_str!("../tests/gallery/camera.json")),
        ("cat", include_str!("../tests/gallery/cat.json")),
        ("cloud", include_str!("../tests/gallery/cloud.json")),
        ("egg", include_str!("../tests/gallery/egg.json")),
        ("ghost", include_str!("../tests/gallery/ghost.json")),
        ("magnifier", include_str!("../tests/gallery/magnifier.json")),
        ("paint-brush", include_str!("../tests/gallery/paint-brush.json")),
        ("palette", include_str!("../tests/gallery/palette.json")),
        ("rocket", include_str!("../tests/gallery/rocket.json")),
        ("shield", include_str!("../tests/gallery/shield.json")),
        ("star", include_str!("../tests/gallery/star.json")),
        ("witch-hat", include_str!("../tests/gallery/witch-hat.json")),
    ];
    for (name, raw) in files {
        let gallery: serde_json::Value = serde_json::from_str(raw).expect("JSON");
        assert!(validate_avatar(&gallery).is_ok(), "{name} is valid");
        let ours =
            if name == "shield" || name == "star" { engine_body(name) } else { engine_body(&format!("obj-{name}")) }
                .unwrap_or_else(|| panic!("{name} is in the catalog"));
        let eyes = &gallery["eyes"];
        let want = ["x", "y", "spacing", "scale"].map(|k| eyes[k].as_f64().expect("number"));
        assert_eq!(gallery["id"], name);
        assert_eq!(ours.name, gallery["name"].as_str().expect("name"), "{name} name");
        assert_eq!(ours.body_path, gallery["body"]["path"].as_str().expect("path"), "{name} path");
        assert_eq!(eyes_of(ours), want, "{name} eyes");
    }
    for starter in examples() {
        let ours = engine_body(&format!("obj-{}", starter.id)).expect("starter");
        assert_eq!(ours.body_path, starter.body_path);
        assert_eq!(eyes_of(ours), eyes_of(&starter));
    }
}

/// The cursor, every festival costume and anything unknown have no engine definition. Plain
/// "ghost", "cat" and "rocket" are festival costumes; the objects only answer to "obj-ghost" etc.
#[test]
fn engine_body_is_none_for_the_cursor_and_the_festivals() {
    let festival = [
        "pumpkin", "ghost", "witch", "bat", "cat", "skull", "monster", "mummy", "vampire", "spider", "rocket", "anaar",
        "chakri", "sparkler", "bomb", "laddu", "chakli", "jalebi", "katli", "karanji",
    ];
    for id in ["cursor"].into_iter().chain(festival) {
        assert!(engine_body(id).is_none(), "{id} has no engine definition");
    }
    for id in ["", "obj-", "obj-cursor", "obj-nothing", "Blob", "BLOB", "obj-Cat", " blob", "toString", "__proto__"] {
        assert!(engine_body(id).is_none(), "{id:?}");
    }
    assert!(engine_body("blob").is_some() && engine_body("obj-cat").is_some());
}

/// Every body passes the contract validation and renders to SVG and to a frame.
#[test]
fn every_body_validates_and_renders() {
    for d in object_bodies().into_iter().chain(shape_bodies()) {
        let json = serde_json::json!({
            "version": d.version, "id": d.id, "name": d.name, "body": { "path": d.body_path },
            "eyes": { "x": d.eyes.x, "y": d.eyes.y, "spacing": d.eyes.spacing, "scale": d.eyes.scale },
        });
        let checked = validate_avatar(&json);
        assert!(checked.is_ok(), "{}: {:?}", d.id, checked.err());
        let options = RenderOptions { size: Some(64.0), ..RenderOptions::default() };
        let svg = render_avatar_svg(&d, &options).unwrap_or_else(|e| panic!("{}: {e:?}", d.id));
        assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>"), "{}", d.id);
        assert!(avatar_frame(&d, &options).is_ok(), "{}", d.id);
    }
}

/// `MAUS_COLORS` has the twelve colours, hexes as in src/lib/mascot.ts:51.
#[test]
fn maus_colors_are_the_apps() {
    let names: Vec<&str> = MAUS_COLORS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        ["green", "blue", "red", "orange", "purple", "cyan", "pink", "yellow", "teal", "coral", "silver", "chrome"]
    );
    assert_eq!(MAUS_COLORS[0].1, "#009957");
    assert_eq!(MAUS_COLORS[11].1, "#B3BAC4");
}

/// `vivid` lifts saturation 15%. The expected values come from an independent HLS calculation
/// (Python `colorsys`) of the formula in CarvedAvatar.tsx:123: green #009957 is already fully
/// saturated and comes back unchanged; blue #377FE6 goes from s 0.7778 to 0.8944; red #D94B52 from
/// 0.6514 to 0.7491.
#[test]
fn vivid_lifts_saturation_by_fifteen_percent() {
    assert_eq!(vivid("#009957"), "#009957");
    assert_eq!(vivid("#377FE6"), "#2a7df3");
    assert_eq!(vivid("#D94B52"), "#e44048");
    // Greys have no saturation to lift and come back as given.
    assert_eq!(vivid("#808080"), "#808080");
    assert_eq!(vivid("#FFFFFF"), "#FFFFFF");
    // The app only passes hexes; anything else comes back as it was rather than being guessed at.
    assert_eq!(vivid("nope"), "nope");
    for (_, hex) in MAUS_COLORS {
        let v = vivid(hex);
        assert!(v.len() == 7 && v.starts_with('#'), "{hex} -> {v}");
    }
}

/// `customFill(bodyColor) ?? vivid(MAUS_COLORS[color] ?? MAUS_COLORS.green)` (Avatar.tsx:141).
#[test]
fn body_fill_prefers_a_custom_colour_then_the_vivid_palette_then_green() {
    assert_eq!(body_fill(Some("#AbCdEf"), "red"), "#abcdef");
    assert_eq!(body_fill(None, "blue"), vivid("#377FE6"));
    assert_eq!(body_fill(Some("red"), "blue"), vivid("#377FE6"));
    assert_eq!(body_fill(Some("#abc"), "blue"), vivid("#377FE6"));
    assert_eq!(body_fill(Some("#abcdeg"), "blue"), vivid("#377FE6"));
    assert_eq!(body_fill(Some(""), "blue"), vivid("#377FE6"));
    assert_eq!(body_fill(None, "unknown"), body_fill(None, "green"));
    assert_eq!(custom_fill(Some("#00FF7f")).as_deref(), Some("#00ff7f"));
    assert_eq!(custom_fill(Some("#00FF7f ")), None);
    assert_eq!(custom_fill(None), None);
}

/// `mascotFaceInk`: white and black only.
#[test]
fn face_ink_is_white_or_black_or_automatic() {
    assert_eq!(face_ink(Some("white")), Some("#ffffff"));
    assert_eq!(face_ink(Some("black")), Some("#000000"));
    for other in [None, Some(""), Some("White"), Some("red"), Some("#ffffff")] {
        assert_eq!(face_ink(other), None, "{other:?}");
    }
}

/// Regenerates `development/oba-samples/shape-*.svg`, one per shape, filled as the app fills a
/// bot (`body_fill`) with the eye ink of `face_ink`; `development/oba-qt-check.cpp` paints them
/// with QtSvg. Run on demand with
/// `cargo test --manifest-path engine/Cargo.toml catalog::tests::shape_samples -- --ignored`.
#[test]
#[ignore = "writes files under development/oba-samples/"]
fn shape_samples_are_written() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../development/oba-samples");
    std::fs::create_dir_all(&dir).expect("create oba-samples");
    let colors = ["blue", "green", "orange", "purple", "pink", "teal", "red", "cyan", "yellow"];
    let inks = [None, None, Some("black"), Some("white"), None, Some("white"), None, Some("black"), None];
    for (i, d) in shape_bodies().iter().enumerate() {
        let options = RenderOptions {
            color: Some(body_fill(None, colors[i])),
            eye_color: face_ink(inks[i]).map(String::from),
            size: Some(256.0),
            time: Some(1.1),
            ..RenderOptions::default()
        };
        let svg = render_avatar_svg(d, &options).expect("renders");
        assert!(svg.starts_with("<svg "), "{}", d.id);
        std::fs::write(dir.join(format!("shape-{}.svg", d.id)), svg).expect("write sample");
    }
}
