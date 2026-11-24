//! Ported from upstream `avatar.test.ts`. The test that compares the starters with the skill's
//! example files and the system prompt reads files of the upstream repository (`skills/`), which
//! are not part of this port, so it is not ported.

use super::*;
use crate::avatar::oba::color::parse_hex;
use crate::avatar::oba::motion::{PoseInput, blink_at, compute_pose};
use crate::avatar::oba::path::{PathError, flatten_path, is_inside, parse_path};
use crate::avatar::oba::render::{DEFAULT_COLOR, DEFAULT_EYE_COLOR, render_avatar_svg};
use crate::avatar::oba::validate::{RECESS_MAX_AREA, analyze_body};
use serde_json::{Value, json};

fn controller() -> AvatarDefinition {
    ex("controller")
}

/// Minimal well-formedness check: every opened tag closes in order (avatar.test.ts `assertBalanced`).
fn assert_balanced(svg: &str) {
    let mut stack: Vec<String> = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find('<') {
        let end = rest[start..].find('>').expect("tag closes") + start;
        let tag = &rest[start + 1..end];
        rest = &rest[end + 1..];
        if tag.ends_with('/') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            assert_eq!(stack.pop().as_deref(), Some(name));
        } else {
            stack.push(tag.split(|c: char| c.is_whitespace()).next().unwrap_or("").to_string());
        }
    }
    assert!(stack.is_empty(), "unclosed: {stack:?}");
}

fn with(mut v: Value, key: &str, value: Value) -> Value {
    v[key] = value;
    v
}

/// avatar.test.ts "examples include controller and barbell and all validate".
#[test]
fn avatar_examples_include_controller_and_barbell_and_all_validate() {
    let all = examples();
    let ids: Vec<&str> = all.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"controller") && ids.contains(&"barbell"));
    let unique: std::collections::HashSet<&&str> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
    for e in &all {
        assert_eq!(errors_of(&to_json(e)), Vec::<String>::new(), "{}", e.id);
    }
}

/// avatar.test.ts "normalization is idempotent".
#[test]
fn avatar_normalization_is_idempotent() {
    for e in examples() {
        let once = validate_avatar(&to_json(&e)).expect("valid");
        let twice = validate_avatar(&to_json(&once)).expect("valid again");
        assert_eq!(twice, once, "{}", e.id);
    }
}

/// avatar.test.ts "cuts counter-wound holes and merges overlapping outlines".
#[test]
fn avatar_path_cuts_counter_wound_holes_and_merges_overlapping_outlines() {
    let mug = flatten_path(&parse_path(&ex("mug").body_path).unwrap());
    assert!(!is_inside(&mug, 72.0, 53.0), "inside the handle loop");
    assert!(is_inside(&mug, 43.0, 52.0), "cup face");
    let barbell = flatten_path(&parse_path(&ex("barbell").body_path).unwrap());
    assert!(is_inside(&barbell, 30.0, 50.0), "bar/plate overlap stays filled");
}

/// avatar.test.ts "classifies holes by size and keeps per-subpath probes".
#[test]
fn avatar_path_classifies_holes_by_size_and_keeps_per_subpath_probes() {
    let buttons = analyze_body("M10 20H90V80H10ZM48 48A2 2 0 1 0 52 48A2 2 0 1 0 48 48Z").unwrap();
    assert_eq!(buttons.holes.len(), 1);
    assert!(buttons.holes[0].area <= RECESS_MAX_AREA);
    let mug = analyze_body(&ex("mug").body_path).unwrap();
    assert_eq!(mug.holes.len(), 1);
    assert!(mug.holes[0].area > RECESS_MAX_AREA);
    let hole = mug.subpaths.iter().find(|s| s.hole).expect("a hole subpath");
    let probe = hole.probe.expect("a probe");
    assert!(!is_inside(&mug.polys, probe[0], probe[1]));
}

/// avatar.test.ts "expands relative and shorthand commands to absolute geometry".
#[test]
fn avatar_path_expands_relative_and_shorthand_commands() {
    let a = flatten_path(&parse_path("M20 20h60v60h-60z").unwrap());
    let b = flatten_path(&parse_path("M20 20L80 20L80 80L20 80Z").unwrap());
    assert_eq!(a, b);
    let bad: Result<_, PathError> = parse_path("M0 0 A5 5 0 011 10 10");
    assert!(bad.is_err());
}

/// avatar.test.ts "rejects markup, styling and unknown fields".
#[test]
fn avatar_validate_rejects_markup_styling_and_unknown_fields() {
    let c = to_json(&controller());
    let mut script = c.clone();
    script["body"]["path"] = json!("M10 10L90 10L90 90Z\"/><script>alert(1)</script>");
    assert!(errors_of(&script).join(",").contains("unsupported character"));
    assert!(errors_of(&with(c.clone(), "color", json!("#f00"))).join(",").contains("unknown field \"color\""));
    let mut fill = c.clone();
    fill["body"]["fill"] = json!("red");
    assert!(errors_of(&fill).join(",").contains("body.fill"));
    assert!(!errors_of(&Value::Null).is_empty());
    assert!(errors_of(&with(c.clone(), "version", json!(2))).join(",").contains("version"));
    assert!(errors_of(&with(c, "id", json!("Has Spaces"))).join(",").contains("id"));
}

fn with_path(path: &str) -> Value {
    let mut v = to_json(&controller());
    v["body"]["path"] = json!(path);
    v
}

/// avatar.test.ts "rejects geometry outside the safe area or too small".
#[test]
fn avatar_validate_rejects_geometry_outside_the_safe_area_or_too_small() {
    assert!(errors_of(&with_path("M2 20L98 20L98 80L2 80Z")).join(",").contains("safe area"));
    assert!(errors_of(&with_path("M45 40L55 40L55 52L45 52Z")).join(",").contains("too small"));
    assert!(errors_of(&with_path("M10 10Q50 50 90 10X")).join(",").contains("unsupported character"));
}

/// avatar.test.ts "rejects empty and whitespace-only paths".
#[test]
fn avatar_validate_rejects_empty_and_whitespace_only_paths() {
    for path in ["", "  \n\t "] {
        assert_eq!(errors_of(&with_path(path)), vec!["body.path: path is empty".to_string()]);
    }
}

/// avatar.test.ts "requires eyes on solid fill, not off-body or in a hole".
#[test]
fn avatar_validate_requires_eyes_on_solid_fill_not_off_body_or_in_a_hole() {
    let mut off = to_json(&controller());
    off["eyes"]["y"] = json!(70);
    assert!(errors_of(&off).join(",").contains("eyes must sit inside"));
    let mut on_handle = to_json(&ex("mug"));
    on_handle["eyes"] = json!({ "x": 66, "y": 53, "spacing": 12, "scale": 1 });
    assert!(errors_of(&on_handle).join(",").contains("outside or in a hole"));
    let mut crowded = to_json(&controller());
    crowded["eyes"]["spacing"] = json!(4);
    assert!(errors_of(&crowded).join(",").contains("spacing"));
}

/// avatar.test.ts "produces balanced standalone SVG for every example and motion". Adapted: the
/// `<svg>` tag also declares `xmlns:xlink` (QtSvg needs it for `<use>`), so the prefix is checked
/// in two parts.
#[test]
fn avatar_svg_is_balanced_standalone_for_every_example_and_motion() {
    for e in examples() {
        for m in [AvatarMotion::Idle, AvatarMotion::Thinking, AvatarMotion::Happy] {
            let s = svg(&e, &RenderOptions { size: Some(96.0), ..motion(m, 1.7) });
            assert!(s.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""), "{}", e.id);
            assert!(s.contains(" viewBox=\"0 0 100 100\" width=\"96\" height=\"96\""), "{}", e.id);
            assert_balanced(&s);
            for bad in ["NaN", "Infinity", "undefined"] {
                assert!(!s.contains(bad), "{} {bad}", e.id);
            }
        }
    }
}

/// avatar.test.ts "is deterministic in time and frozen under reduced motion".
#[test]
fn avatar_svg_is_deterministic_in_time_and_frozen_under_reduced_motion() {
    let c = controller();
    assert_eq!(svg(&c, &at(2.5)), svg(&c, &at(2.5)));
    assert_ne!(svg(&c, &at(0.5)), svg(&c, &at(1.5)));
    let calm = svg(&c, &RenderOptions { time: Some(0.5), reduced_motion: true, ..opts() });
    assert_eq!(svg(&c, &RenderOptions { time: Some(9.0), reduced_motion: true, ..opts() }), calm);
    assert_ne!(svg(&c, &RenderOptions { reduced_motion: true, gaze: gaze(1.0, 0.0), ..opts() }), calm);
}

/// avatar.test.ts "never echoes untrusted option or name text into markup".
#[test]
fn avatar_svg_never_echoes_untrusted_option_or_name_text() {
    let mut d = controller();
    d.name = "Pad & \"Co\"".into();
    let s = svg(
        &d,
        &RenderOptions {
            color: Some("red\" onload=\"x".into()),
            eye_color: Some("url(javascript:1)".into()),
            ..opts()
        },
    );
    assert!(!s.contains("onload") && !s.contains("javascript"));
    assert!(s.contains("aria-label=\"Pad &#38; &#34;Co&#34;\""));
    assert!(s.contains(DEFAULT_COLOR));
}

/// avatar.test.ts "throws on invalid definitions instead of rendering them" (an `Err` here).
#[test]
fn avatar_svg_errs_on_invalid_definitions() {
    let mut d = controller();
    d.body_path = "M0 0<g/>".into();
    let e = render_avatar_svg(&d, &opts()).unwrap_err();
    assert!(e.contains("Invalid avatar"), "{e}");
}

/// Every `fill="…"` and `stroke="…"` value in a string.
fn paints(svg: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for key in ["fill=\"", "stroke=\""] {
        let mut rest = svg;
        while let Some(i) = rest.find(key) {
            // Skip `stroke-width="` style attributes: they do not match `stroke="`.
            let after = &rest[i + key.len()..];
            let v = after.split('"').next().unwrap_or("");
            if v != "none" {
                out.insert(v.to_string());
            }
            rest = after;
        }
    }
    out
}

/// avatar.test.ts "draws one flat body color and plain dark eyes: no gradients, gloss, glints or
/// shadows". Upstream's mask exclusion is not needed: this renderer writes no masks.
#[test]
fn avatar_svg_draws_one_flat_body_color_and_plain_dark_eyes() {
    for e in examples() {
        for color in ["#ff6a1a", "#f4f4f4", "#7a7a7a"] {
            let s = svg(&e, &RenderOptions { color: Some(color.into()), ..at(1.1) });
            let lower = s.to_lowercase();
            for bad in ["gradient", "<filter", "<ellipse", "<circle", "opacity", "<mask"] {
                assert!(!lower.contains(bad), "{} {bad}", e.id);
            }
            let mut want: Vec<String> = vec![color.to_string(), DEFAULT_EYE_COLOR.to_string()];
            want.sort();
            assert_eq!(paints(&s).into_iter().collect::<Vec<_>>(), want, "{} {color}", e.id);
        }
    }
}

/// avatar.test.ts "turning reveals volume: the drawn stack shifts with the view, flat depth does not".
#[test]
fn avatar_turning_reveals_volume() {
    let c = controller();
    let at_yaw = |yaw: f64, depth: Option<f64>| {
        svg(&c, &RenderOptions { reduced_motion: true, view: view(yaw, 0.0), depth, ..opts() })
    };
    assert_ne!(at_yaw(0.6, None), at_yaw(-0.6, None));
    let slices = |s: &str| s.matches("<use ").count();
    assert!(slices(&at_yaw(0.9, None)) > slices(&at_yaw(0.0, Some(0.0))));
    assert_eq!(slices(&at_yaw(0.9, Some(0.0))), 1);
}

/// avatar.test.ts "hides the eyes once the front turns away". The eyes are the `<rect>` elements
/// (MausBot's smile is a stroked path, so the count is upstream's).
#[test]
fn avatar_hides_the_eyes_once_the_front_turns_away() {
    let c = controller();
    let eyes = |yaw: f64| {
        svg(&c, &RenderOptions { reduced_motion: true, view: view(yaw, 0.0), ..opts() }).matches("<rect ").count()
    };
    assert_eq!(eyes(0.0), 2);
    assert_eq!(eyes(std::f64::consts::PI), 0);
}

/// avatar.test.ts "blinks briefly and periodically": 20 s at 30 fps; closed (> 0.8) under 8% of frames.
#[test]
fn avatar_motion_blinks_briefly_and_periodically() {
    let frames: Vec<f64> = (0..30 * 20).map(|i| blink_at(42, i as f64 / 30.0)).collect();
    let closed = frames.iter().filter(|b| **b > 0.8).count();
    assert!(closed > 0);
    assert!((closed as f64 / frames.len() as f64) < 0.08);
}

/// avatar.test.ts "clamps gaze and keeps poses finite".
#[test]
fn avatar_motion_clamps_gaze_and_keeps_poses_finite() {
    let p = compute_pose(&PoseInput {
        seed: 1,
        time: 3.0,
        motion: Some(AvatarMotion::Idle),
        gaze: gaze(50.0, -50.0),
        reduced_motion: true,
        ..PoseInput::default()
    });
    assert_eq!(p.gaze_x, 1.0);
    assert_eq!(p.gaze_y, -1.0);
    for m in [AvatarMotion::Idle, AvatarMotion::Thinking, AvatarMotion::Happy] {
        let mut t = 0.0;
        while t < 6.0 {
            let q = compute_pose(&PoseInput { seed: 9, time: t, motion: Some(m), ..PoseInput::default() });
            let nums = [
                q.yaw,
                q.pitch,
                q.roll,
                q.lift,
                q.sx,
                q.sy,
                q.gaze_x,
                q.gaze_y,
                q.blink,
                q.squint_l,
                q.squint_r,
                q.eyes.arc,
                q.eyes.slit,
                q.ribbons,
                q.cage,
                q.sweep,
                q.bang,
                q.dots,
            ];
            assert!(nums.iter().all(|v| v.is_finite()), "{m:?} {t}");
            assert!(q.roll.abs() < 10.0);
            assert!(q.lift <= 0.0);
            t += 0.37;
        }
    }
}

/// The colour check `parse_hex` the renderer uses for options: a bad value is not echoed.
#[test]
fn avatar_default_colors_parse() {
    assert!(parse_hex(DEFAULT_COLOR).is_some());
    assert!(parse_hex(DEFAULT_EYE_COLOR).is_some());
}
