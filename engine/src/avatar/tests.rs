use serde_json::json;

use super::mausbot::mascot::{self, Look, Part};
use super::oba::catalog::{self, SHAPE_BODY_IDS};
use super::oba::render::avatar_extent;
use super::*;

fn look_of(bot: serde_json::Value, status: &str, cheering: bool) -> Look {
    let mut look = mascot::bot_look(&bot, status, None).expect("look");
    look.paint.cheering = cheering;
    look
}

fn festival_ids() -> Vec<String> {
    let data: serde_json::Value = serde_json::from_str(include_str!("mausbot/festival.json")).expect("festival.json");
    data["bodies"].as_object().expect("bodies").keys().cloned().collect()
}

fn is_oba(body: &str) -> bool {
    matches!(painter(body), Painter::Oba(_))
}

#[test]
fn a_body_is_painted_as_avatar_tsx_decides_carved_then_engine_then_cursor() {
    // Avatar.tsx:117 isCarvedBody, :136 engineBody, :152 GradientAvatar.
    let festivals = festival_ids();
    assert_eq!(festivals.len(), 20);
    for id in &festivals {
        assert!(matches!(painter(id), Painter::Festival), "{id} is carved");
    }
    for id in SHAPE_BODY_IDS {
        assert!(is_oba(id), "{id} is an engine body");
    }
    let objects: Vec<&str> = catalog::objects::OBJECT_BODY_IDS.to_vec();
    assert_eq!(objects.len(), 21);
    for id in objects {
        assert!(is_oba(id), "{id} is an engine body");
    }
    for id in ["cursor", "", "robot", "OBJ-bulb", "not-a-body"] {
        assert!(matches!(painter(id), Painter::Cursor), "{id:?} falls through to the cursor");
    }
}

#[test]
fn the_roster_accepts_every_bot_body_and_no_obj_falls_back_to_the_cursor() {
    let mut bodies = vec!["cursor".to_string()];
    bodies.extend(festival_ids());
    bodies.extend(SHAPE_BODY_IDS.map(String::from));
    bodies.extend(catalog::objects::OBJECT_BODY_IDS.map(String::from));
    assert_eq!(bodies.len(), 1 + 20 + 9 + 21);
    for body in &bodies {
        let look = mascot::bot_look(&json!({"mascotBody": body, "color": "blue"}), "idle", None).expect("look");
        assert_eq!(&look.body, body, "{body} is kept");
    }
    let unknown = mascot::bot_look(&json!({"mascotBody": "obj-nothing"}), "idle", None).expect("look");
    assert_eq!(unknown.body, "cursor", "only a body nobody knows becomes the cursor");
}

#[test]
fn the_old_shapes_are_gone_from_the_cursor_catalogue() {
    let data = mascot::catalogue().expect("catalogue");
    let bodies: Vec<&String> = data["bodies"].as_object().expect("bodies").keys().collect();
    assert_eq!(bodies, ["cursor"]);
}

#[test]
fn an_engine_body_is_one_picture_with_its_eyes_inside_it() {
    let look = look_of(json!({"mascotBody": "obj-lightbulb", "color": "blue"}), "idle", false);
    let body = avatar_svg(&look, Part::Body).expect("body");
    assert!(body.starts_with("<svg"), "{body}");
    assert_eq!(avatar_svg(&look, Part::Eyes), None, "no second picture to turn");
    assert_eq!(eye_pivot(&look), None);
    assert_eq!(variant_count(&look), 0);
    assert!(body.contains("width=\"96\" height=\"96\""));
}

#[test]
fn the_picture_is_framed_by_the_avatars_extent_so_qt_does_not_cut_the_ribbons() {
    for id in ["obj-lightbulb", "star", "obj-controller"] {
        let Painter::Oba(definition) = painter(id) else { panic!("{id}") };
        let extent = avatar_extent(definition, None).expect("extent");
        assert!(extent.size > 100.0, "{id}: the ribbons reach beyond the 100 box ({})", extent.size);
        let look = look_of(json!({"mascotBody": id, "color": "blue"}), "idle", true);
        let body = avatar_svg(&look, Part::Body).expect("body");
        let expected = format!(
            "viewBox=\"{} {} {} {}\"",
            oba::path::fmt(extent.x),
            oba::path::fmt(extent.y),
            oba::path::fmt(extent.size),
            oba::path::fmt(extent.size)
        );
        assert!(body.contains(&expected), "{id}: wanted {expected} in {}", &body[..body.len().min(260)]);
        assert!(!body.contains("viewBox=\"0 0 100 100\""), "{id}");
    }
}

#[test]
fn the_extent_is_the_same_whatever_the_state_so_a_bot_never_changes_size() {
    let view_box = |face: &str, cheering: bool| {
        let svg = avatar_svg(
            &look_of(json!({"mascotBody": "obj-lightbulb", "mascotExpression": face}), "idle", cheering),
            Part::Body,
        )
        .expect("body");
        svg.split("viewBox=\"").nth(1).and_then(|rest| rest.split('"').next()).expect("viewBox").to_string()
    };
    let rest = view_box("idle", false);
    for (face, cheering) in [("working", false), ("thinking", false), ("sad", false), ("idle", true)] {
        assert_eq!(view_box(face, cheering), rest, "{face} cheering={cheering}");
    }
}

#[test]
fn the_picture_at_rest_is_still_and_the_moving_one_is_a_frame_that_changes_with_time() {
    // Phase 4: `avatar_svg` is the picture the bar shows when nothing moves it, drawn `still` (reduced
    // motion). A bot that is active or cheering gets frames from `oba_frame_svg` instead.
    let bulb = |face: &str, cheering: bool| {
        // A bot's own `working` face goes with a busy bot: the app animates a bot only then.
        let status = if face == "working" { "working" } else { "idle" };
        look_of(json!({"mascotBody": "obj-lightbulb", "mascotExpression": face}), status, cheering)
    };
    let ribbons = |svg: &str| svg.matches("linearGradient").count();
    let definition = catalog::engine_body("obj-lightbulb").expect("the bulb is an engine body");

    for (face, cheering) in [("idle", false), ("idle", true), ("working", false)] {
        let still = avatar_svg(&bulb(face, cheering), Part::Body).expect("body");
        assert_eq!(ribbons(&still), 0, "{face} cheering={cheering}: a picture at rest carries no ribbons");
    }
    assert_eq!(
        avatar_svg(&bulb("working", false), Part::Body),
        avatar_svg(&bulb("working", false), Part::Body),
        "the still picture does not depend on the clock"
    );

    assert!(oba_frame_svg(definition, &bulb("idle", false), 1.0, None).is_none(), "an idle bot has no frames");
    let cheering = |age: f64| oba_frame_svg(definition, &bulb("idle", true), age, None).expect("a cheering bot moves");
    assert!(ribbons(&cheering(1.935)) > 0, "a cheering bulb is out with its ribbons mid-turn");
    assert_ne!(cheering(1.0), cheering(1.1), "the frames change with time");
    assert_eq!(cheering(1.0), cheering(1.0), "a frame is a function of its age alone");
    let working = oba_frame_svg(definition, &bulb("working", false), 1.0, None).expect("a working bot moves");
    assert_ne!(working, cheering(1.0), "working is not done");
    let sweeps = (0..80).any(|step| {
        let frame = oba_frame_svg(definition, &bulb("working", false), f64::from(step) * 0.1, None);
        frame.is_some_and(|svg| ribbons(&svg) > 0)
    });
    assert!(sweeps, "a working bulb sweeps its ribbons now and then");
}

#[test]
fn the_body_is_filled_and_inked_as_the_app_does() {
    let paint = |bot: serde_json::Value| avatar_svg(&look_of(bot, "idle", false), Part::Body).expect("body");
    let vivid_blue = catalog::vivid("#377FE6");
    assert!(paint(json!({"mascotBody": "circle", "color": "blue"})).contains(&format!("fill=\"{vivid_blue}\"")));
    assert!(
        paint(json!({"mascotBody": "circle", "color": "blue", "mascotBodyColor": "#AABBCC"}))
            .contains("fill=\"#aabbcc\""),
        "a custom body colour wins"
    );
    assert!(paint(json!({"mascotBody": "circle", "color": "blue", "mascotBodyColor": "red"})).contains(&vivid_blue));
    let white = paint(json!({"mascotBody": "circle", "color": "blue", "mascotFaceColor": "white"}));
    assert!(white.contains("#ffffff"));
    let black = paint(json!({"mascotBody": "circle", "color": "blue", "mascotFaceColor": "black"}));
    assert!(black.contains("#000000"));
    assert_ne!(white, black);
}

#[test]
fn the_cursor_and_the_festivals_are_still_drawn_by_mausbot() {
    let cursor = look_of(json!({"color": "blue"}), "idle", false);
    assert_eq!(avatar_svg(&cursor, Part::Body), mascot::avatar_svg(&cursor, Part::Body));
    assert!(avatar_svg(&cursor, Part::Eyes).is_some());
    assert_eq!(eye_pivot(&cursor), mascot::eye_pivot(&cursor));
    let ghost = look_of(json!({"mascotBody": "ghost", "color": "blue"}), "idle", false);
    assert_eq!(avatar_svg(&ghost, Part::Body), festival::avatar_svg(&ghost, Part::Body));
    assert_eq!(avatar_svg(&ghost, Part::Eyes), festival::avatar_svg(&ghost, Part::Eyes));
    assert_eq!(eye_pivot(&ghost), festival::eye_pivot(&ghost));
}

/// Regenerates `development/oba-samples/phase3-lightbulb-done.svg` (and the old 0 0 100 100 framing next
/// to it, `phase3-lightbulb-done-clipped.svg`): `development/oba-qt-check` paints both with QtSvg.
/// `cargo test --manifest-path engine/Cargo.toml avatar::tests::phase3_samples -- --ignored`.
#[test]
#[ignore = "writes files under development/oba-samples/"]
fn phase3_samples_are_written() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../development/oba-samples");
    std::fs::create_dir_all(&dir).expect("create oba-samples");
    let look =
        look_of(json!({"mascotBody": "obj-lightbulb", "color": "blue", "mascotFaceColor": "white"}), "idle", true);
    let framed = avatar_svg(&look, Part::Body).expect("body");
    let clipped = {
        let Painter::Oba(definition) = painter("obj-lightbulb") else { panic!("bulb") };
        let extent = avatar_extent(definition, None).expect("extent");
        framed.replacen(
            &format!(
                "viewBox=\"{} {} {} {}\"",
                oba::path::fmt(extent.x),
                oba::path::fmt(extent.y),
                oba::path::fmt(extent.size),
                oba::path::fmt(extent.size)
            ),
            "viewBox=\"0 0 100 100\"",
            1,
        )
    };
    std::fs::write(dir.join("phase3-lightbulb-done.svg"), framed).expect("write");
    std::fs::write(dir.join("phase3-lightbulb-done-clipped.svg"), clipped).expect("write");
}

/// Writes the pictures Mission publishes for each of the 30 engine bodies, at rest (`idle`, what
/// `avatar_svg` gives) and working (the frame `oba_frame_svg` draws 1 s into the state), to
/// `development/avatars-engine-port/svg/`. `development/avatars-engine-port-check.py` compares them
/// with the app's own tables and rules. Every bot is blue with the default face ink.
/// `cargo test --manifest-path engine/Cargo.toml avatar::tests::port_comparison_svgs -- --ignored`.
#[test]
#[ignore = "writes files under development/avatars-engine-port/svg/"]
fn port_comparison_svgs_are_written() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../development/avatars-engine-port/svg");
    std::fs::create_dir_all(&dir).expect("create svg dir");
    let ids: Vec<&str> = SHAPE_BODY_IDS.iter().copied().chain(catalog::objects::OBJECT_BODY_IDS).collect();
    assert_eq!(ids.len(), 30);
    let mut facts = serde_json::Map::new();
    for id in ids {
        let bot = json!({"mascotBody": id, "color": "blue"});
        let idle = look_of(bot.clone(), "idle", false);
        std::fs::write(dir.join(format!("{id}.idle.svg")), avatar_svg(&idle, Part::Body).expect("idle"))
            .expect("write");
        let working = look_of(bot, "working", false);
        let Painter::Oba(definition) = painter(id) else { panic!("{id} is an engine body") };
        let frame = oba_frame_svg(definition, &working, 1.0, None).expect("a working body moves");
        std::fs::write(dir.join(format!("{id}.working.svg")), frame).expect("write");
        let eyes = &definition.eyes;
        facts.insert(
            id.to_string(),
            json!({"name": definition.name, "path": definition.body_path, "eyes": [eyes.x, eyes.y, eyes.spacing, eyes.scale]}),
        );
    }
    let facts = serde_json::to_string_pretty(&facts).expect("json");
    std::fs::write(dir.join("../definitions.json"), facts).expect("write definitions");
}

#[test]
fn an_idle_researcher_is_a_still_picture_and_only_moves_while_it_works() {
    // The spin VK saw: Researcher (drop, no pinned face) rests on the `searching` face from its
    // title, which is in ACTIVE_STATES. The bar read that as "moving"; the app's sidebar passes
    // `animated={beat || working || noticing}` (Sidebar.tsx:742), so it held the bot still.
    let bot = json!({"name": "Researcher", "title": "Research Specialist", "mascotBody": "drop", "color": "purple"});
    let definition = catalog::engine_body("drop").expect("drop is an engine body");
    let idle = look_of(bot.clone(), "idle", false);
    assert_eq!(idle.expression, "searching", "the resting face comes from the title");
    assert!(!looks::shown(&idle.expression, false, Some(idle.paint.working)).live);
    assert!(oba_frame_svg(definition, &idle, 1.0, None).is_none(), "an idle researcher has no frames");
    let busy = look_of(bot, "working", false);
    assert!(oba_frame_svg(definition, &busy, 1.0, None).is_some(), "a busy researcher moves");
}

#[test]
fn gaze_moves_the_eyes_of_an_engine_body_even_at_rest() {
    let bot = json!({"mascotBody": "obj-lightbulb", "color": "blue"});
    let definition = catalog::engine_body("obj-lightbulb").expect("bulb is an engine body");
    let idle = look_of(bot.clone(), "idle", false);
    let at = |look: &Look, gaze| oba_frame_svg(definition, look, 1.0, gaze);
    assert!(at(&idle, None).is_none(), "an idle bot with no gaze has no frame");
    let centre = at(&idle, Some([0.0, 0.0])).expect("a gaze draws a bot at rest");
    let left = at(&idle, Some([-1.0, 0.0])).expect("left");
    let right = at(&idle, Some([1.0, 0.5])).expect("right");
    assert_ne!(centre, left, "the eyes turn left");
    assert_ne!(left, right, "and right");
    let working = look_of(bot, "working", false);
    assert_ne!(at(&working, Some([1.0, 0.0])), at(&working, Some([-1.0, 0.0])), "a moving bot looks too");
}
