//! Ported from upstream `gallery.test.ts`. Its seams check (`seams.ts`) is not ported with this
//! module, so it is not run here.

use super::*;
use crate::avatar::oba::validate::analyze_body;
use crate::avatar::oba::volume::face_room;

const NAMES: [&str; 16] = [
    "book",
    "brain",
    "bull",
    "bunny",
    "camera",
    "cat",
    "cloud",
    "egg",
    "ghost",
    "magnifier",
    "paint-brush",
    "palette",
    "rocket",
    "shield",
    "star",
    "witch-hat",
];

/// gallery.test.ts "every gallery avatar is valid, named after its file, and not a starter".
#[test]
fn gallery_avatars_are_valid_named_after_their_file_and_not_starters() {
    let starters: Vec<String> = examples().into_iter().map(|e| e.id).collect();
    assert_eq!(gallery().len(), NAMES.len());
    for (name, (file, def)) in NAMES.iter().zip(gallery()) {
        assert_eq!(file, format!("{name}.json"));
        assert_eq!(def.id, *name, "id matches the file name");
        assert!(!starters.contains(&def.id), "{name} is not a starter");
    }
}

/// gallery.test.ts "every gallery avatar gives its face room".
#[test]
fn gallery_avatars_give_their_face_room() {
    for (file, def) in gallery() {
        let body = analyze_body(&def.body_path).expect("body");
        let room = face_room(&def, &body);
        assert!(room.ok, "{file}: boost {} clearance {} gaze {:?}", room.boost, room.clearance, room.gaze);
    }
}
