//! Regenerates the sample SVGs in `development/oba-samples/`, which `development/oba-qt-check.cpp`
//! paints with QtSvg. Not a check of values (the other modules are): run it on demand with
//! `cargo test --manifest-path engine/Cargo.toml oba_samples -- --ignored`.

use super::*;

/// Writes one SVG per look: the starters at rest, a working bot mid-sweep, a celebration, thinking,
/// blocked, and a turned view with extra depth.
#[test]
#[ignore = "writes files under development/oba-samples/"]
fn oba_samples_are_written() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../development/oba-samples");
    std::fs::create_dir_all(&dir).expect("create oba-samples");
    let work = |t: f64| RenderOptions {
        state: Some(AvatarState::Working),
        time: Some(t),
        state_age: Some(t),
        size: Some(256.0),
        ..opts()
    };
    let samples: Vec<(&str, String)> = vec![
        ("controller-idle", svg(&ex("controller"), &RenderOptions { size: Some(256.0), ..at(1.1) })),
        ("mug-working", svg(&ex("mug"), &work(1.8))),
        (
            "lightbulb-done",
            svg(
                &ex("lightbulb"),
                &RenderOptions {
                    state: Some(AvatarState::Done),
                    time: Some(1.35),
                    state_age: Some(1.35),
                    size: Some(256.0),
                    ..opts()
                },
            ),
        ),
        ("barbell-happy", svg(&ex("barbell"), &RenderOptions { size: Some(256.0), ..happy_at(2.0) })),
        (
            "chat-thinking",
            svg(&ex("chat"), &RenderOptions { size: Some(256.0), ..state_at(AvatarState::Thinking, 0.8) }),
        ),
        (
            "sprout-blocked",
            svg(&ex("sprout"), &RenderOptions { size: Some(256.0), ..state_at(AvatarState::Blocked, 0.8) }),
        ),
        (
            "watering-can-turned",
            svg(
                &ex("watering-can"),
                &RenderOptions {
                    size: Some(256.0),
                    view: view(0.9, 0.2),
                    depth: Some(10.0),
                    color: Some("#2f9e44".into()),
                    ..at(0.5)
                },
            ),
        ),
    ];
    for (name, s) in samples {
        assert!(s.starts_with("<svg "), "{name}");
        std::fs::write(dir.join(format!("{name}.svg")), s).expect("write sample");
    }
}
