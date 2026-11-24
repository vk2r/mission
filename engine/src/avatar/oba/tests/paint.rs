//! Adapted from upstream `paint.test.ts`. That suite tests `dom/paint.ts` (`framePlacement`,
//! `fitSquare`, `liveFrame` fitting for the canvas), which is not part of this port. What carries
//! over is the idea behind its last test, "holds frames at any view and depth within the extent":
//! everything a frame paints stays inside `avatar_extent`, the square the SVG's `viewBox` could use
//! to show the avatar at every view and pose. The painted points are computed as upstream's
//! `painted()` does (every vertex of every flattened slice, the corners of ribbon boxes and eye
//! pills, each through its matrix), now also through the frame's `root` since the SVG draws every
//! shape inside it.

use super::*;
use crate::avatar::oba::path::{flatten_path, parse_path_unbounded};
use crate::avatar::oba::render::{AvatarFrame, Matrix, avatar_extent};
use crate::avatar::oba::types::{AvatarMotionBlend, BlendFrom, BlendLook};
use std::collections::HashMap;

/// `a` applied after `b`.
fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// Every corner of every polyline vertex and eye shape, placed: what the canvas actually paints.
fn painted(frame: &AvatarFrame, flat: &mut HashMap<String, Vec<f64>>) -> Vec<Point> {
    let paths: HashMap<&str, &str> = frame.paths.iter().map(|p| (p.id.as_str(), p.d.as_str())).collect();
    let mut points = Vec::new();
    for s in &frame.shapes {
        let m = mul(&frame.root, &matrix_of(s));
        let local: Vec<f64> = match s {
            FrameShape::Slice { path, .. } => {
                let d = paths.get(path.as_str()).copied().unwrap_or("");
                flat.entry(d.to_string())
                    .or_insert_with(|| {
                        flat_points(&flatten_path(&parse_path_unbounded(d).expect("a rendered path parses")))
                            .into_iter()
                            .flatten()
                            .collect()
                    })
                    .clone()
            }
            FrameShape::Ribbon { bbox, .. } => vec![bbox.x, bbox.y, bbox.x + bbox.w, bbox.y + bbox.h],
            FrameShape::Eye { mark: EyeMark::Pill { x, y, width, height, .. }, .. } => {
                vec![*x, *y, x + width, y + height]
            }
            FrameShape::Eye { .. } => vec![],
        };
        for k in (0..local.len()).step_by(2) {
            points.push([m[0] * local[k] + m[2] * local[k + 1] + m[4], m[1] * local[k] + m[3] * local[k + 1] + m[5]]);
        }
    }
    points
}

fn blend_from_working_happy(t: f64) -> Option<AvatarMotionBlend> {
    Some(AvatarMotionBlend {
        from: BlendFrom::Look(BlendLook {
            state: Some(AvatarState::Working),
            mood: Some(AvatarMood::Happy),
            ..BlendLook::default()
        }),
        t,
    })
}

/// The poses upstream's `poses` list builds: the three legacy motions with a wandering gaze or a
/// turned view, then five states with a mood, an age, a blend and a view.
fn poses() -> Vec<RenderOptions> {
    let mut out = Vec::new();
    for m in [AvatarMotion::Idle, AvatarMotion::Happy, AvatarMotion::Thinking] {
        for t in 0..12 {
            let tf = t as f64;
            out.push(RenderOptions {
                motion: Some(m),
                time: Some(tf * 0.61),
                gaze: if t % 3 != 0 { gaze(tf.sin(), (tf * 1.3).cos()) } else { None },
                ..opts()
            });
            out.push(RenderOptions {
                motion: Some(m),
                time: Some(tf * 0.37),
                view: view(tf * 0.9 - 5.0, tf.sin() * 1.4),
                ..opts()
            });
        }
    }
    for state in
        [AvatarState::Working, AvatarState::Waiting, AvatarState::Blocked, AvatarState::Thinking, AvatarState::Done]
    {
        for t in 0..8 {
            let tf = t as f64;
            out.push(RenderOptions {
                state: Some(state),
                mood: Some(if t % 2 == 1 { AvatarMood::Happy } else { AvatarMood::Neutral }),
                time: Some(tf * 0.43),
                state_age: Some(tf * 0.2),
                gaze: if t % 3 != 0 { gaze(tf.cos(), tf.sin()) } else { None },
                ..opts()
            });
            out.push(RenderOptions {
                state: Some(state),
                time: Some(tf * 0.29),
                state_age: Some(tf * 0.17),
                motion_blend: blend_from_working_happy(tf / 8.0),
                view: view(tf - 4.0, tf.cos()),
                ..opts()
            });
        }
    }
    out
}

/// paint.test.ts "holds frames at any view and depth within the extent": no painted point leaves
/// `avatar_extent`, for every starter, depths 0, 6 and 14, and every pose above.
#[test]
fn paint_holds_frames_at_any_view_and_depth_within_the_extent() {
    let mut flat = HashMap::new();
    let all_poses = poses();
    assert_eq!(all_poses.len(), 3 * 12 * 2 + 5 * 8 * 2);
    for d in examples() {
        for depth in [0.0, 6.0, 14.0] {
            let extent = avatar_extent(&d, Some(depth)).expect("extent");
            for o in &all_poses {
                let f = frame(&d, &RenderOptions { depth: Some(depth), ..o.clone() });
                for [x, y] in painted(&f, &mut flat) {
                    assert!(
                        x >= extent.x - 1e-9
                            && y >= extent.y - 1e-9
                            && x <= extent.x + extent.size + 1e-9
                            && y <= extent.y + extent.size + 1e-9,
                        "{} depth {depth}: ({x}, {y}) outside {extent:?} in {o:?}",
                        d.id
                    );
                }
            }
        }
    }
}

/// paint.test.ts "bounds everything a frame paints, to well within the antialiasing pad" (adapted):
/// the first 18 poses at the default depth keep a margin inside the extent, not just inside it.
#[test]
fn paint_leaves_a_margin_inside_the_extent_at_the_default_depth() {
    let mut flat = HashMap::new();
    for d in examples() {
        let extent = avatar_extent(&d, None).expect("extent");
        for o in poses().iter().take(18) {
            for [x, y] in painted(&frame(&d, o), &mut flat) {
                assert!(
                    x > extent.x && y > extent.y && x < extent.x + extent.size && y < extent.y + extent.size,
                    "{}",
                    d.id
                );
            }
        }
    }
}
