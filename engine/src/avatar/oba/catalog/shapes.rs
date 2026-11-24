//! Our nine shapes (`SHAPE_BODIES` in the app's object-bodies.ts), drawn for the same engine.
//! Shield and Star are Open Bot Avatar's own gallery avatars; the cursor keeps its own renderer.

use super::super::types::AvatarDefinition;
use super::definition;

/// In the order of `MASCOT_BODY_IDS` without the cursor, as (id, name, path, eyes x/y/spacing/scale).
const SHAPES: [(&str, &str, &str, [f64; 4]); 9] = [
    (
        "blob",
        "Blob",
        "M48 10C64 8 78 16 85 30C92 44 90 62 80 75C70 88 52 93 36 87C20 81 9 66 10 49C11 30 30 12 48 10Z",
        [50.0, 48.0, 18.0, 1.15],
    ),
    ("circle", "Circle", "M50 10A40 40 0 1 1 50 90A40 40 0 1 1 50 10Z", [50.0, 48.0, 18.0, 1.15]),
    (
        "squircle",
        "Squircle",
        "M50 12C78 12 88 22 88 50C88 78 78 88 50 88C22 88 12 78 12 50C12 22 22 12 50 12Z",
        [50.0, 48.0, 18.0, 1.15],
    ),
    (
        "capsule",
        "Capsule",
        "M50 8C66 8 78 20 78 36V64C78 80 66 92 50 92C34 92 22 80 22 64V36C22 20 34 8 50 8Z",
        [50.0, 46.0, 16.0, 1.1],
    ),
    (
        "drop",
        "Drop",
        "M46.5 12Q50 7 53.5 12C64 27 80 45 80 62C80 79 66 91 50 91C34 91 20 79 20 62C20 45 36 27 46.5 12Z",
        [50.0, 60.0, 16.0, 1.1],
    ),
    (
        "shield",
        "Shield",
        "M50 14C60 18 72 18 82 16Q86 16 86 20L86 46C86 64 72 78 54 86.5Q50 88.5 46 86.5C28 78 14 64 14 46L14 20Q14 16 18 16C28 18 40 18 50 14Z",
        [50.0, 42.0, 17.0, 1.15],
    ),
    (
        "hexagon",
        "Hexagon",
        "M40.47 13.5Q50 8 59.53 13.5L76.85 23.5Q86.37 29 86.37 40L86.37 60Q86.37 71 76.85 76.5L59.53 86.5Q50 92 40.47 86.5L23.15 76.5Q13.63 71 13.63 60L13.63 40Q13.63 29 23.15 23.5Z",
        [50.0, 47.0, 18.0, 1.15],
    ),
    (
        "diamond",
        "Diamond",
        "M40.1 16.9Q50 7 59.9 16.9L83.1 40.1Q93 50 83.1 59.9L59.9 83.1Q50 93 40.1 83.1L16.9 59.9Q7 50 16.9 40.1Z",
        [50.0, 47.0, 16.0, 1.1],
    ),
    (
        "star",
        "Star",
        "M53.7 17.95L62 31.2Q64.1 34.6 68 35.6L83.1 39.3Q89.9 41 85.4 46.4L75.4 58.3Q72.8 61.4 73.1 65.4L74.2 81Q74.7 88 68.2 85.4L53.7 79.5Q50 78 46.3 79.5L31.8 85.4Q25.3 88 25.8 81L26.9 65.4Q27.2 61.4 24.6 58.3L14.6 46.4Q10.1 41 16.9 39.3L32 35.6Q35.9 34.6 38 31.2L46.3 17.95Q50 12 53.7 17.95Z",
        [50.0, 52.0, 16.0, 1.1],
    ),
];

/// `SHAPE_BODIES` as a list in picker order.
pub fn shape_bodies() -> Vec<AvatarDefinition> {
    SHAPES.iter().map(|(id, name, path, eyes)| definition(id, name, path, *eyes)).collect()
}
