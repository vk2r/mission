//! Ported from upstream `volume.test.ts`. Not ported: "live scene" (`renderAvatarScene` and
//! `sceneFrame`, the CSS scene) and "silhouette seams" (`seams.ts`), which are not part of this port.

use super::*;
use crate::avatar::oba::contour::{Field, contour_loops, loops_to_path, simplify_loop};
use crate::avatar::oba::path::{
    Bounds, FLATNESS, filled_area, flatten_path, inside_row, is_inside, parse_path, winding,
};
use crate::avatar::oba::render::{Stack, active_stack, plane_matrix, project_point, side_plane_matrix, view_basis};
use crate::avatar::oba::types::AvatarEyes;
use crate::avatar::oba::validate::{EYE_CLEARANCE, EYE_H, GAZE_X, GAZE_Y, analyze_body, drawn_eyes_fit, gaze_range};
use crate::avatar::oba::volume::{
    EYE_BOOST, EYE_BOOST_PAD, FACE_ROOM_TARGET, PartKind, VolumePart, build_volume_of, erosion_at, face_room,
    revolve_axis, surface_z,
};
use serde_json::json;

fn def(path: &str, eyes: (f64, f64, f64, f64)) -> AvatarDefinition {
    let v = json!({
        "version": 1, "id": "test", "name": "Test", "body": { "path": path },
        "eyes": { "x": eyes.0, "y": eyes.1, "spacing": eyes.2, "scale": eyes.3 },
    });
    validate_avatar(&v).unwrap_or_else(|e| panic!("{}", e.join("; ")))
}

const CENTER_EYES: (f64, f64, f64, f64) = (50.0, 50.0, 14.0, 1.0);

fn volume_of(e: &AvatarDefinition) -> crate::avatar::oba::volume::Volume {
    build_volume_of(e, 1.0).expect("volume")
}

fn path_polys(d: &str) -> Vec<Vec<Point>> {
    flatten_path(&parse_path(d).unwrap())
}

fn height(b: &Bounds) -> f64 {
    b.max_y - b.min_y
}

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} vs {b} (tolerance {tol})");
}

/// volume.test.ts "gives every example finite, closed, nonzero 3D extent in both slice stacks".
#[test]
fn volume_gives_every_example_finite_closed_nonzero_extent_in_both_stacks() {
    for e in examples() {
        let vol = volume_of(&e);
        assert!(!vol.parts.is_empty(), "{}", e.id);
        for part in &vol.parts {
            assert!(part.depth > 2.0, "{}", e.id);
            assert!(part.z_slices.len() > 4, "{}", e.id);
            assert!(part.x_slices.len() > 2, "{}", e.id);
            for s in part.z_slices.iter().chain(&part.x_slices) {
                assert!(numbers(&s.d).iter().all(|n| n.is_finite()));
                assert!(s.d.ends_with('Z'), "every loop closed");
                assert!(s.bbox.w > 0.0);
            }
            // Front and back are mirror images; the stack spans most of the depth.
            let zs: Vec<f64> = part.z_slices.iter().map(|s| s.at).collect();
            let (lo, hi) = (
                zs.iter().cloned().fold(f64::INFINITY, f64::min),
                zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            );
            near(lo + hi, 0.0, 1e-6);
            assert!(hi - lo > part.depth * 0.6);
            let side_tolerance = (1.1_f64).max((part.bounds.max_x - part.bounds.min_x) / 40.0) / 2.0 + 0.45;
            for s in &part.x_slices {
                assert!(s.at >= part.bounds.min_x - side_tolerance);
                assert!(s.at <= part.bounds.max_x + side_tolerance);
                assert!(s.bbox.w / 2.0 <= part.depth / 2.0 + 0.2);
            }
        }
        assert!(vol.eye_z.iter().all(|z| *z > 0.0));
    }
}

/// volume.test.ts "rounds inflated edges: cross-sections shrink toward front and back, never at the core".
#[test]
fn volume_rounds_inflated_edges() {
    let vol = volume_of(&ex("controller"));
    let part = &vol.parts[0];
    assert_eq!(part.kind, PartKind::Inflate);
    assert_eq!(erosion_at(part.depth, part.rxy, part.rz, 0.0), 0.0);
    let mut prev = 0.0;
    let mut z = 0.0;
    while z <= part.depth / 2.0 {
        let e = erosion_at(part.depth, part.rxy, part.rz, z);
        assert!(e >= prev - 1e-9);
        prev = e;
        z += part.depth / 40.0;
    }
    near(surface_z(part.depth, part.rxy, part.rz, part.rxy + 1.0), part.depth / 2.0, 1e-9);
    assert!(surface_z(part.depth, part.rxy, part.rz, 0.5) < part.depth / 2.0 - 1.0);
    // Outer slices are nested inside the core outline and pulled in from it.
    let outer = part.z_slices.last().unwrap();
    assert!(outer.inset > 2.0);
    let core = path_polys(&part.d);
    for [x, y] in path_polys(&outer.d).iter().flatten() {
        assert!(is_inside(&core, *x, *y));
    }
}

/// volume.test.ts "handles concavity and holes: the controller notch and the mug handle loop stay empty".
#[test]
fn volume_handles_concavity_and_holes() {
    let controller = volume_of(&ex("controller"));
    let c = &controller.parts[0];
    assert_eq!(c.height(50.0, 70.0), 0.0, "between the grips");
    assert!(c.height(50.0, 45.0) > 5.0);
    let mug = volume_of(&ex("mug"));
    let handle = mug.parts.iter().find(|p| p.kind == PartKind::Inflate).expect("a handle");
    assert!(!is_inside(&handle.polys, 72.0, 53.0));
    // Contour slices are drawn with evenodd, the core with the original nonzero rule.
    for s in &handle.z_slices {
        let w = winding(&path_polys(&s.d), 72.0, 53.0);
        let filled = if s.even_odd { w.abs() % 2 == 1 } else { w != 0 };
        assert!(!filled);
    }
    // A ring is as deep as its band is thick, not as deep as its bounding box is wide.
    assert!(handle.depth < 18.0);
}

/// volume.test.ts "turns symmetric parts of multi-part bodies into solids of revolution".
#[test]
fn volume_turns_symmetric_parts_into_solids_of_revolution() {
    let mug = volume_of(&ex("mug"));
    let cup = mug.parts.iter().find(|p| p.kind == PartKind::RevolveY).expect("a revolved cup");
    near(cup.depth, cup.bounds.max_x - cup.bounds.min_x, 0.5);
    let r = cup.depth / 2.0;
    let axis = cup.axis.expect("axis");
    near(cup.height(axis, 50.0), r, 0.5);
    near(cup.height(axis + r * std::f64::consts::FRAC_1_SQRT_2, 50.0), r * std::f64::consts::FRAC_1_SQRT_2, 0.5);
    assert_eq!(volume_of(&ex("controller")).parts[0].kind, PartKind::Inflate);
    assert_eq!(volume_of(&ex("chat")).parts[0].kind, PartKind::Inflate);
}

/// volume.test.ts "reads the symmetric barbell as round discs on a cylinder about one horizontal axis".
#[test]
fn volume_reads_the_barbell_as_round_discs_on_a_cylinder() {
    let body_of = |id: &str| analyze_body(&ex(id).body_path).unwrap();
    near(revolve_axis(&body_of("barbell")).expect("axis"), 50.0, 1e-6);
    let vol = volume_of(&ex("barbell"));
    assert!(vol.parts.iter().all(|p| p.kind == PartKind::RevolveX && p.axis == Some(50.0)));
    let plate = vol.parts.iter().find(|p| is_inside(&p.polys, 26.0, 20.0)).expect("a plate");
    near(plate.depth, height(&plate.bounds), 0.5);
    let side =
        plate.x_slices.iter().reduce(|a, b| if a.bbox.w * a.bbox.h > b.bbox.w * b.bbox.h { a } else { b }).unwrap();
    near(side.bbox.w, side.bbox.h, 0.5);
    let bar = &vol.parts[vol.eye_part];
    assert!(plate.depth > bar.depth * 1.5, "discs much wider than the grip cylinder");
    assert!(revolve_axis(&body_of("watering-can")).is_none());
    assert!(revolve_axis(&body_of("controller")).is_none());
}

/// volume.test.ts "scales with the volume amount and goes flat at 0".
#[test]
fn volume_scales_with_the_volume_amount_and_goes_flat_at_0() {
    let c = ex("controller");
    let one = build_volume_of(&c, 1.0).unwrap().parts[0].depth;
    near(build_volume_of(&c, 2.0).unwrap().parts[0].depth, (one * 2.0).min(60.0), 1e-6);
    let flat = build_volume_of(&c, 0.0).unwrap();
    assert_eq!(flat.parts[0].z_slices.len(), 1);
    assert_eq!(flat.parts[0].x_slices.len(), 0);
}

/// volume.test.ts "enlarges the eyes where the body has room and seats them above the surface".
#[test]
fn volume_enlarges_the_eyes_where_the_body_has_room() {
    let bx = volume_of(&def("M15 25H85V75H15Z", CENTER_EYES));
    near(bx.eye_scale, EYE_BOOST, 1e-6);
    near(bx.eye_z[0], bx.parts[0].depth / 2.0, 0.05);
    for e in examples() {
        let vol = volume_of(&e);
        assert!(vol.eye_scale >= e.eyes.scale * 1.15, "{}", e.id);
        let host = &vol.parts[vol.eye_part];
        assert!(vol.eye_z[0] >= host.height(e.eyes.x - e.eyes.spacing / 2.0, e.eyes.y) - 1e-9, "{}", e.id);
    }
}

/// volume.test.ts "traces closed loops of a sampled disc, including a hole".
#[test]
fn contours_trace_closed_loops_of_a_sampled_disc_including_a_hole() {
    let n = 41;
    let mut values = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..n {
            let r = ((i as f64 - 20.0).powi(2) + (j as f64 - 20.0).powi(2)).sqrt();
            values[j * n + i] = if r < 6.0 { -1.0 } else { 15.0 - r }; // ring between radius 6 and 15
        }
    }
    let loops = contour_loops(&Field { values, nx: n, ny: n, x0: 0.0, y0: 0.0, step: 1.0 }, 0.0);
    assert_eq!(loops.len(), 2);
    let mut radii: Vec<f64> = loops
        .iter()
        .map(|l| l.iter().map(|[x, y]| ((x - 20.0).powi(2) + (y - 20.0).powi(2)).sqrt()).sum::<f64>() / l.len() as f64)
        .collect();
    radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
    near(radii[1], 15.0, 0.5);
    assert!(radii[0] > 5.0);
    assert!(simplify_loop(&loops[0], 0.05).len() <= loops[0].len());
}

/// volume.test.ts "scanline inside tests agree with per-point winding".
#[test]
fn contours_scanline_inside_tests_agree_with_per_point_winding() {
    let polys = path_polys(&ex("mug").body_path);
    let xs: Vec<f64> = (0..90).map(|i| 5.3 + i as f64).collect();
    let mut row = vec![0u8; xs.len()];
    for y in [30.1, 53.2, 70.7] {
        inside_row(&polys, y, &xs, &mut row);
        for (k, x) in xs.iter().enumerate() {
            assert_eq!(row[k] == 1, is_inside(&polys, *x, y), "({x}, {y})");
        }
    }
}

/// volume.test.ts "is the identity for the front view and keeps the pivot fixed".
#[test]
fn projection_is_the_identity_for_the_front_view_and_keeps_the_pivot_fixed() {
    let pivot = [50.0, 50.0];
    let v = view_basis(0.0, 0.0);
    assert_eq!(plane_matrix(&v, 0.0, pivot), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(project_point(&v, [30.0, 70.0, 5.0], pivot), [30.0, 70.0, 5.0]);
    let [x, y, _] = project_point(&view_basis(0.7, 0.3), [50.0, 50.0, 0.0], pivot);
    near(x, 50.0, 1e-9);
    near(y, 50.0, 1e-9);
}

/// volume.test.ts "turns about the vertical axis: front points move right and toward the viewer
/// for positive yaw".
#[test]
fn projection_turns_about_the_vertical_axis() {
    let pivot = [50.0, 50.0];
    let [x, _, depth] = project_point(&view_basis(std::f64::consts::FRAC_PI_4, 0.0), [50.0, 50.0, 10.0], pivot);
    near(x, 50.0 + 10.0 * std::f64::consts::FRAC_1_SQRT_2, 1e-9);
    near(depth, 10.0 * std::f64::consts::FRAC_1_SQRT_2, 1e-9);
    assert!(project_point(&view_basis(std::f64::consts::PI, 0.0), [50.0, 50.0, -10.0], pivot)[2] > 9.0);
}

/// volume.test.ts "maps side planes consistently with front planes and switches stacks at 45°".
#[test]
fn projection_maps_side_planes_consistently_and_switches_stacks_at_45_degrees() {
    let pivot = [50.0, 50.0];
    let v = view_basis(1.1, 0.4);
    for [x, y, z] in [[20.0, 80.0, -12.0], [60.0, 30.0, 7.0]] {
        let m = side_plane_matrix(&v, x, pivot);
        let [px, py, _] = project_point(&v, [x, y, z], pivot);
        // Side planes use local u = -z.
        near(m[0] * -z + m[2] * y + m[4], px, 1e-9);
        near(m[1] * -z + m[3] * y + m[5], py, 1e-9);
    }
    assert_eq!(active_stack(&view_basis(0.7, 0.0)), Stack::Z);
    assert_eq!(active_stack(&view_basis(0.9, 0.0)), Stack::X);
    assert_eq!(active_stack(&view_basis(std::f64::consts::PI, 0.0)), Stack::Z);
}

const CORNERS: [[f64; 2]; 8] =
    [[-1.0, -1.0], [-1.0, 0.0], [-1.0, 1.0], [0.0, -1.0], [0.0, 1.0], [1.0, -1.0], [1.0, 0.0], [1.0, 1.0]];

/// volume.test.ts "every starter keeps visible room around the eyes as drawn: enlarged, happy arcs,
/// any gaze".
#[test]
fn face_room_every_starter_keeps_visible_room_around_the_eyes_as_drawn() {
    for e in examples() {
        let body = analyze_body(&e.body_path).unwrap();
        let room = face_room(&e, &body);
        assert!(room.ok && room.boost == EYE_BOOST, "{}", e.id);
        assert!(room.clearance >= FACE_ROOM_TARGET, "{}", e.id);
        // Eye offsets exactly as the renderer computes them, at every gaze extreme.
        let scale = volume_of(&e).eye_scale;
        let eyes = AvatarEyes { scale, ..e.eyes };
        let range = gaze_range(&body.polys, &eyes);
        for [gx, gy] in CORNERS {
            let dx = gx * if gx >= 0.0 { range.right } else { range.left } * GAZE_X * scale;
            let dy = gy * if gy >= 0.0 { range.down } else { range.up } * GAZE_Y * scale;
            assert!(drawn_eyes_fit(&body.polys, &eyes, dx, dy, EYE_CLEARANCE).is_none(), "{} gaze {gx},{gy}", e.id);
        }
    }
}

/// volume.test.ts "the part carrying the face is the largest part of every starter".
#[test]
fn face_room_the_part_carrying_the_face_is_the_largest_part() {
    for e in examples() {
        let vol = volume_of(&e);
        let areas: Vec<usize> = vol.parts.iter().map(|p| filled_area(&p.polys, p.bounds)).collect();
        assert_eq!(*areas.iter().max().unwrap(), areas[vol.eye_part], "{}", e.id);
    }
}

struct Extents {
    hollow: bool,
    box_ratio: f64,
    tall: f64,
    out: f64,
}

/// How big each secondary part looks next to the face part (upstream `extents`).
fn extents(e: &AvatarDefinition) -> Vec<Extents> {
    let vol = volume_of(e);
    let f = vol.parts[vol.eye_part].bounds;
    let (fw, fh) = (f.max_x - f.min_x, f.max_y - f.min_y);
    // A member that runs right through the face part (a shaft) is structure, not an add-on.
    let through = |b: &Bounds| (b.min_x < f.min_x && b.max_x > f.max_x) || (b.min_y < f.min_y && b.max_y > f.max_y);
    vol.parts
        .iter()
        .enumerate()
        .filter(|(i, p)| *i != vol.eye_part && !through(&p.bounds))
        .map(|(_, p)| {
            let b = p.bounds;
            let (w, h) = (b.max_x - b.min_x, b.max_y - b.min_y);
            Extents {
                hollow: p.polys.len() > 1,
                box_ratio: (w * h) / (fw * fh),
                tall: h / fh,
                out: [
                    0.0,
                    (f.min_x - b.min_x) / fw,
                    (b.max_x - f.max_x) / fw,
                    (f.min_y - b.min_y) / fh,
                    (b.max_y - f.max_y) / fh,
                ]
                .into_iter()
                .fold(0.0, f64::max),
            }
        })
        .collect()
}

fn subordinate(e: &AvatarDefinition) -> bool {
    extents(e)
        .iter()
        .all(|a| a.box_ratio <= 0.5 && (!a.hollow || (a.box_ratio <= 0.3 && a.tall <= 0.7 && a.out <= 0.35)))
}

/// volume.test.ts "accessories stay subordinate by outer extent, not just by filled area".
#[test]
fn face_room_accessories_stay_subordinate_by_outer_extent() {
    for e in examples() {
        assert!(subordinate(&e), "{}", e.id);
    }
    // The earlier watering can, whose thin handle arch towered over the body.
    let arch_can = def(
        concat!(
            "M24 44H56C62 44 65 47 66 52L68 76C68.5 82 64 86 58 86H22C16 86 11.5 82 12 76L14 52C15 47 18 44 24 44Z",
            "M14 58C8 50 7 36 12 28C17 20 29 19 36 25C41 29 43 36 43 46ZM35 44C35 36 31 29 25 29C19 29 16 34 16 40C16 43 17 45 18 47Z",
            "M64 60C70 55 75 47 79 35.5L84 38.5C80 52 73 66 66 76ZM77.8 31.3A7.5 3.4 34 0 1 90.2 39.7A7.5 3.4 34 0 1 77.8 31.3Z",
        ),
        (40.0, 64.0, 16.0, 1.15),
    );
    let vol = volume_of(&arch_can);
    let areas: Vec<usize> = vol.parts.iter().map(|p| filled_area(&p.polys, p.bounds)).collect();
    assert_eq!(*areas.iter().max().unwrap(), areas[vol.eye_part]);
    assert!(!subordinate(&arch_can));
}

/// Share of a handle opening's edge that is the face part rather than the handle band (upstream `closesOnFace`).
fn closes_on_face(e: &AvatarDefinition) -> Vec<f64> {
    let vol = volume_of(e);
    let host = &vol.parts[vol.eye_part].polys;
    let all = analyze_body(&e.body_path).unwrap().polys;
    let mut out = Vec::new();
    for (i, p) in vol.parts.iter().enumerate() {
        if i == vol.eye_part || p.polys.len() <= 1 {
            continue;
        }
        let (mut on_host, mut edge) = (0.0, 0.0);
        let b = p.bounds;
        let mut x = b.min_x;
        while x <= b.max_x {
            let mut y = b.min_y;
            while y <= b.max_y {
                if is_inside(&p.polys[..1], x, y) && !is_inside(&all, x, y) {
                    for [dx, dy] in [[0.75, 0.0], [-0.75, 0.0], [0.0, 0.75], [0.0, -0.75]] {
                        if !is_inside(&all, x + dx, y + dy) {
                            continue;
                        }
                        edge += 1.0;
                        if is_inside(host, x + dx, y + dy) {
                            on_host += 1.0;
                        }
                    }
                }
                y += 0.5;
            }
            x += 0.5;
        }
        out.push(on_host / edge);
    }
    out
}

/// volume.test.ts "shrunken handle loops stay attached: their opening closes against the face part".
#[test]
fn face_room_shrunken_handle_loops_stay_attached() {
    for id in ["mug", "watering-can"] {
        let shares = closes_on_face(&ex(id));
        assert_eq!(shares.len(), 1, "{id}");
        assert!(shares[0] > 0.2, "{id}: {}", shares[0]);
    }
    let body = "M32 38H62C68 38 71 41 72 47L74 76C74.5 82 70 86 64 86H30C24 86 19.5 82 20 76L22 47C23 41 26 38 32 38Z";
    let ring = "M12.8 50.2A12.5 9 -55 0 1 27.2 29.8A12.5 9 -55 0 1 12.8 50.2ZM15.7 46.1A7.5 4 -55 0 0 24.3 33.9A7.5 4 -55 0 0 15.7 46.1Z";
    assert!(closes_on_face(&def(&format!("{body}{ring}"), (47.0, 61.0, 16.0, 1.15)))[0] < 0.1);
}

/// volume.test.ts "barbell: a grip just big enough for the face, stepping down through two weight
/// tiers to a bar end".
#[test]
fn face_room_barbell_steps_down_through_two_weight_tiers_to_a_bar_end() {
    let b = ex("barbell");
    let vol = volume_of(&b);
    let grip = &vol.parts[vol.eye_part];
    let h = |p: &VolumePart| height(&p.bounds);
    let mut right: Vec<&VolumePart> = vol
        .parts
        .iter()
        .enumerate()
        .filter(|(i, p)| *i != vol.eye_part && p.bounds.min_x > 50.0)
        .map(|(_, p)| p)
        .collect();
    right.sort_by(|p, q| p.bounds.min_x.partial_cmp(&q.bounds.min_x).unwrap());
    assert_eq!(right.len(), 3);
    let (big, small, end) = (right[0], right[1], right[2]);
    assert!(h(big) > h(grip) * 1.5);
    assert!(h(small) < h(big) * 0.8);
    assert!(h(end) < h(small) * 0.6);
    assert!(h(end) >= 15.0, "a chunky bar end, not a pin");
    assert!(end.bounds.max_x - small.bounds.max_x >= 5.0);
    let body = analyze_body(&b.body_path).unwrap();
    assert!(face_room(&b, &body).clearance < FACE_ROOM_TARGET * 2.0);
    assert!((EYE_H * vol.eye_scale) / 2.0 + GAZE_Y * vol.eye_scale < (h(grip) / 2.0) * 0.65);
}

/// volume.test.ts "gaze is a small nudge, not a slide to the edge".
#[test]
#[allow(clippy::assertions_on_constants)]
fn face_room_gaze_is_a_small_nudge() {
    assert!(GAZE_X * EYE_BOOST <= 2.5);
    assert!(GAZE_Y * EYE_BOOST <= 2.0);
}

/// volume.test.ts "still accepts tight v1 faces: they validate and render, and are only flagged by
/// the face-room check".
#[test]
fn face_room_still_accepts_tight_v1_faces() {
    let tight = def("M20 39H80A6 6 0 0 1 86 45V55A6 6 0 0 1 80 61H20A6 6 0 0 1 14 55V45A6 6 0 0 1 20 39Z", CENTER_EYES);
    let body = analyze_body(&tight.body_path).unwrap();
    assert!(!face_room(&tight, &body).ok);
    assert!(render_ok(&tight));
    let vol = volume_of(&tight);
    if vol.eye_scale > tight.eyes.scale {
        let eyes = AvatarEyes { scale: vol.eye_scale, ..tight.eyes };
        assert!(drawn_eyes_fit(&body.polys, &eyes, 0.0, 0.0, EYE_BOOST_PAD).is_none());
    }
}

fn render_ok(d: &AvatarDefinition) -> bool {
    crate::avatar::oba::render::render_avatar_svg(d, &opts()).is_ok()
}

fn eyes_at(e: &AvatarDefinition, yaw: f64) -> usize {
    svg(e, &RenderOptions { reduced_motion: true, view: view(yaw, 0.0), ..opts() }).matches("<rect ").count()
}

/// volume.test.ts "drops the eye nearer the limb of a round part first instead of letting it hang
/// past the edge".
#[test]
fn eye_visibility_drops_the_eye_nearer_the_limb_of_a_round_part_first() {
    for id in ["mug", "lightbulb", "sprout", "watering-can"] {
        let e = ex(id);
        assert_eq!(eyes_at(&e, 0.0), 2, "{id}");
        let counts = [-1.1, 1.4].map(|yaw| eyes_at(&e, yaw));
        assert!(counts.contains(&1), "{id}: {counts:?}");
    }
}

/// volume.test.ts "keeps both eyes on flat faces until the whole front turns away".
#[test]
fn eye_visibility_keeps_both_eyes_on_flat_faces_until_the_front_turns_away() {
    let c = ex("controller");
    assert_eq!(eyes_at(&c, 0.8), 2);
    assert_eq!(eyes_at(&c, std::f64::consts::PI), 0);
}

/// How far each point's distance to the outline departs from its neighbours' average (upstream
/// `roughness`): the 95th percentile.
fn roughness(lp: &[Point], polys: &[Vec<Point>]) -> f64 {
    let ds: Vec<f64> = lp
        .iter()
        .map(|[x, y]| {
            let mut m = f64::INFINITY;
            for p in polys {
                let mut j = p.len() - 1;
                for i in 0..p.len() {
                    let ([ax, ay], [bx, by]) = (p[j], p[i]);
                    let (dx, dy) = (bx - ax, by - ay);
                    let den = dx * dx + dy * dy;
                    let t = (((x - ax) * dx + (y - ay) * dy) / if den == 0.0 { 1.0 } else { den }).clamp(0.0, 1.0);
                    m = m.min((x - ax - t * dx).hypot(y - ay - t * dy));
                    j = i;
                }
            }
            m
        })
        .collect();
    let n = ds.len();
    let mut r: Vec<f64> = (0..n).map(|i| (ds[i] - (ds[(i + n - 1) % n] + ds[(i + 1) % n]) / 2.0).abs()).collect();
    r.sort_by(|a, b| a.partial_cmp(b).unwrap());
    r[(r.len() as f64 * 0.95).floor() as usize]
}

/// volume.test.ts "follow the outline smoothly instead of the sampling grid".
#[test]
fn cross_sections_follow_the_outline_smoothly_instead_of_the_sampling_grid() {
    let mut all = Vec::new();
    for e in examples() {
        for part in volume_of(&e).parts.iter().filter(|p| p.kind == PartKind::Inflate) {
            let area = (part.bounds.max_x - part.bounds.min_x) * (part.bounds.max_y - part.bounds.min_y);
            for s in part.z_slices.iter().filter(|s| s.at > 0.0) {
                for lp in &s.loops {
                    let xs = lp.iter().map(|p| p[0]);
                    let ys = lp.iter().map(|p| p[1]);
                    let span = |it: &mut dyn Iterator<Item = f64>| {
                        let v: Vec<f64> = it.collect();
                        v.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                            - v.iter().cloned().fold(f64::INFINITY, f64::min)
                    };
                    // Full cross-sections only: thin-part slices are clipped to small boxes with straight edges.
                    if span(&mut xs.clone()) * span(&mut ys.clone()) < 0.1 * area {
                        continue;
                    }
                    all.push(roughness(lp, &part.polys));
                }
            }
        }
    }
    all.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(all[(all.len() as f64 * 0.9).floor() as usize] < 0.08);
}

/// volume.test.ts "keep gentle corners between long edges sharp": edges 40 units long turning 0.4
/// rad; curves through their midpoints would leave the outline by 2 units.
#[test]
fn cross_sections_keep_gentle_corners_between_long_edges_sharp() {
    let lp = vec![[10.0, 30.0], [50.0, 22.0], [90.0, 30.0], [90.0, 70.0], [50.0, 62.0], [10.0, 70.0]];
    assert_eq!(loops_to_path(&[lp]), "M30 26l20-4 40 8 0 40-40-8-40 8 0-40Z");
}

/// volume.test.ts "flatten curves to within FLATNESS of the path": a radius-29 circle as two arcs.
#[test]
fn cross_sections_flatten_curves_to_within_flatness_of_the_path() {
    let polys = path_polys("M21 50A29 29 0 0 1 79 50A29 29 0 0 1 21 50Z");
    let poly = &polys[0];
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let sag = 29.0 - (((a[0] + b[0]) / 2.0 - 50.0).hypot((a[1] + b[1]) / 2.0 - 50.0));
        assert!(sag <= FLATNESS + 1e-9, "chord {i}: {sag}");
    }
}
