//! Ported from upstream `frame.test.ts`.

use super::*;
use crate::avatar::oba::path::{flatten_path, parse_path};
use crate::avatar::oba::render::{avatar_frame, live_frame};

/// The `poses` list of frame.test.ts: reduced motion; happy at 1.3 s; thinking at 2.1 s turned; gaze and turned view.
fn poses() -> Vec<RenderOptions> {
    vec![
        RenderOptions { time: Some(0.0), reduced_motion: true, ..opts() },
        motion(AvatarMotion::Happy, 1.3),
        RenderOptions { view: view(1.4, -0.3), ..motion(AvatarMotion::Thinking, 2.1) },
        RenderOptions { gaze: gaze(-0.8, 0.5), view: view(-2.6, 0.6), ..at(0.4) },
    ]
}

/// frame.test.ts "holds exactly what the SVG still draws, in the same order".
#[test]
fn frame_holds_exactly_what_the_svg_still_draws_in_the_same_order() {
    for d in examples() {
        for o in poses() {
            let f = frame(&d, &o);
            let s = svg(&d, &o);
            // QtSvg writes `href` and `xlink:href` side by side; either gives the same id.
            let uses: Vec<String> =
                s.split("<use href=\"#").skip(1).map(|rest| rest.split('"').next().unwrap_or("").to_string()).collect();
            let slices: Vec<String> = f
                .shapes
                .iter()
                .filter_map(|s| if let FrameShape::Slice { path, .. } = s { Some(path.clone()) } else { None })
                .collect();
            assert_eq!(slices, uses, "{}", d.id);
            // Eyes and ribbons are the `<g transform=` groups after the root's.
            let groups = s.matches("<g transform=").count() - 1;
            let drawn = f.shapes.iter().filter(|s| !matches!(s, FrameShape::Slice { .. })).count();
            assert_eq!(drawn, groups, "{}", d.id);
        }
    }
}

/// frame.test.ts "lists every path it references, once".
#[test]
fn frame_lists_every_path_it_references_once() {
    for d in examples() {
        let f = frame(&d, &poses()[2]);
        let ids: Vec<&str> = f.paths.iter().map(|p| p.id.as_str()).collect();
        let unique: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "{}", d.id);
        for s in &f.shapes {
            if let FrameShape::Slice { path, .. } = s {
                assert!(ids.contains(&path.as_str()), "{} lacks {path}", d.id);
            }
        }
    }
}

fn key_of(s: &FrameShape) -> String {
    match s {
        FrameShape::Slice { key, .. } | FrameShape::Ribbon { key, .. } => key.clone(),
        FrameShape::Eye { key, .. } => key.to_string(),
    }
}

/// frame.test.ts "gives each shape a key that is unique within a frame and means the same shape in every frame".
#[test]
fn frame_keys_are_unique_and_stable() {
    for d in examples() {
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for o in poses() {
            let f = frame(&d, &o);
            let keys: Vec<String> = f.shapes.iter().map(key_of).collect();
            let unique: std::collections::HashSet<&String> = keys.iter().collect();
            assert_eq!(unique.len(), keys.len(), "{}", d.id);
            for s in &f.shapes {
                if let FrameShape::Slice { key, path, .. } = s {
                    assert_eq!(seen.get(key).unwrap_or(path), path, "{} {key}", d.id);
                    seen.insert(key.clone(), path.clone());
                }
            }
        }
    }
}

/// frame.test.ts "is plain data with finite numbers". The JSON round trip becomes a clone
/// comparison: a frame is plain owned data and equal to its copy.
#[test]
fn frame_is_plain_data_with_finite_numbers() {
    let f = frame(&examples()[0], &poses()[1]);
    assert_eq!(f.clone(), f);
    for s in &f.shapes {
        assert!(matrix_of(s).iter().all(|v| v.is_finite()));
    }
    assert!(f.root.iter().all(|v| v.is_finite()));
}

/// frame.test.ts "ignores size: custom renderers get every slice at any size".
#[test]
fn frame_ignores_size() {
    for d in examples() {
        for o in poses() {
            assert_eq!(
                avatar_frame(&d, &RenderOptions { size: Some(24.0), ..o.clone() }).unwrap(),
                avatar_frame(&d, &o).unwrap(),
                "{}",
                d.id
            );
        }
    }
}

/// frame.test.ts "bounds every path with its box": curves are sampled, so control points may lie
/// outside, the outline never does.
#[test]
fn frame_bounds_every_path_with_its_box() {
    for d in examples() {
        for p in frame(&d, &poses()[3]).paths {
            for [x, y] in flat_points(&flatten_path(&parse_path(&p.d).expect("a rendered path parses"))) {
                assert!(x >= p.bbox.x - 0.05 && x <= p.bbox.x + p.bbox.w + 0.05, "{} x {x}", d.id);
                assert!(y >= p.bbox.y - 0.05 && y <= p.bbox.y + p.bbox.h + 0.05, "{} y {y}", d.id);
            }
        }
    }
}

/// frame.test.ts "liveFrame thins slices that would sit under a pixel apart, and only for small sizes".
#[test]
fn live_frame_thins_slices_only_for_small_sizes() {
    let base = poses()[0].clone();
    for d in examples() {
        let all = avatar_frame(&d, &base).unwrap().shapes.len();
        let small = live_frame(&d, &RenderOptions { size: Some(36.0), ..base.clone() }).unwrap().shapes.len();
        let big = live_frame(&d, &RenderOptions { size: Some(400.0), ..base.clone() }).unwrap().shapes.len();
        assert!(small < all, "{}: {small} !< {all}", d.id);
        assert!(big > small, "{}", d.id);
        assert_eq!(live_frame(&d, &base).unwrap(), avatar_frame(&d, &base).unwrap(), "{}", d.id);
    }
}
