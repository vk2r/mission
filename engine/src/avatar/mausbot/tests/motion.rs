//! The checks of `scripts/check-motion.mjs` that exercised `mascot/motion.js`, one test each, with the
//! line of the original they come from.

use std::f64::consts::PI;

use crate::avatar::mausbot::mascot;
use crate::avatar::mausbot::motion::*;

fn near(name: &str, got: f64, want: f64) {
    assert!((got - want).abs() <= 1e-6, "{name}: got {got}, want {want}");
}

fn catalogue_faces() -> Vec<String> {
    let faces = mascot::catalogue().and_then(|catalogue| catalogue.get("faces")).and_then(|faces| faces.as_object());
    faces.expect("mascot.json has faces").keys().cloned().collect()
}

#[test]
fn working_bobs_two_and_a_half_units_and_squashes_on_the_way_down() {
    // check-motion.mjs:17-20
    let up = pose("working", 225.0);
    near("working dy up", up.dy, -2.5);
    near("working sx up", up.sx, 1.0);
    near("working sy up", up.sy, 1.0);
    let down = pose("working", 675.0);
    near("working dy down", down.dy, 2.5);
    near("working sx down", down.sx, 1.11);
    near("working sy down", down.sy, 0.89);
}

#[test]
fn thinking_sways_and_breathes() {
    // check-motion.mjs:21-22
    let p = pose("thinking", 750.0);
    near("thinking rot", p.rot, 1.6);
    near("thinking u", p.u, 1.01);
}

#[test]
fn idle_at_the_start_is_at_rest() {
    // check-motion.mjs:23-24
    let p = pose("idle", 0.0);
    near("idle rest u", p.u, 1.0);
    near("idle rest dx", p.dx, 0.0);
    near("idle rest rot", p.rot, 0.0);
}

#[test]
fn sleeping_leans_two_degrees() {
    // check-motion.mjs:25
    near("sleeping lean", pose("sleeping", 0.0).rot, 2.0);
}

#[test]
fn waking_grows_in_from_92_percent() {
    // check-motion.mjs:26-27
    near("waking enter start", pose("waking", 0.0).u, 0.92);
    near("waking enter done", pose("waking", 700.0).u, 1.0 + 0.03 * (700.0 / 2200.0 * PI * 2.0).sin());
}

#[test]
fn orbit_moves_on_a_circle_of_radius_six() {
    // check-motion.mjs:28-29
    let p = pose("orbit", 800.0);
    near("orbit radius", p.dx.hypot(p.dy), 6.0);
}

#[test]
fn the_frame_rate_follows_how_far_a_face_moves_at_the_size_shown() {
    // check-motion.mjs:31-34
    near("working rate", frame_ms("working", 22.0), 1000.0 / 60.0);
    near("scared rate at 22 px", frame_ms("scared", 22.0), 0.0);
    assert!(frame_ms("scared", 160.0) > 0.0, "scared at 160 px should move");
    near("idle rate at 22 px", frame_ms("idle", 22.0), 0.0);
}

#[test]
fn a_blink_shuts_in_134_ms_and_is_open_again_at_320() {
    // check-motion.mjs:36-39
    near("blink start", blink_open(0.0), 1.0);
    near("blink shut", blink_open(134.4), 0.04);
    near("blink open", blink_open(320.0), 1.0);
    near("blink half-way down", blink_open(67.2), 0.5);
}

#[test]
fn the_pause_between_blinks_spans_the_faces_range_and_a_sleeper_never_blinks() {
    // check-motion.mjs:40-42, with -1 as None
    near("working pause min", blink_pause("working", 0.0).expect("blinks"), 2800.0);
    near("working pause max", blink_pause("working", 1.0).expect("blinks"), 5500.0);
    assert_eq!(blink_pause("sleeping", 0.5), None, "sleeping never blinks");
}

#[test]
fn every_face_of_the_catalogue_has_a_motion_entry() {
    // check-motion.mjs:44-46
    for face in catalogue_faces() {
        assert!(MOTION.iter().any(|(name, _)| *name == face), "face \"{face}\" has no motion entry");
    }
}

#[test]
fn every_blink_entry_has_a_motion_entry() {
    // check-motion.mjs:47-49
    for (face, _) in BLINK {
        assert!(MOTION.iter().any(|(name, _)| *name == face), "blink entry \"{face}\" has no motion entry");
    }
}

#[test]
fn every_face_of_the_catalogue_has_a_dwell_range() {
    // check-motion.mjs:51-53
    for face in catalogue_faces() {
        assert!(FACE_DWELL.iter().any(|(name, _)| *name == face), "no dwell range for face {face}");
    }
}

#[test]
fn the_dwell_spans_the_faces_range_and_an_unknown_face_dwells_like_idle() {
    // check-motion.mjs:54-56
    near("curious dwell low", dwell_ms("curious", 0.0), 1800.0);
    near("curious dwell high", dwell_ms("curious", 1.0), 3200.0);
    near("unknown face falls back to idle", dwell_ms("nope", 0.0), 9000.0);
}

#[test]
fn a_face_the_table_leaves_out_stays_at_rest() {
    // motion.js:66, `MOTION[face] || {}`
    assert_eq!(pose("nope", 1234.0), REST);
    assert_eq!(frame_ms("nope", 160.0), 0.0);
}
