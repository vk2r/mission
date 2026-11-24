//! The pictures that move: what the engine computes between two snapshots while the bar shows its
//! avatars, and sends as lines of their own (`{"v":1,"frames":{...}}`, one entry per bot that moves).
//!
//! An engine body is drawn anew each frame, 30 times a second, from the formula of its state; a cursor
//! or costume is a still picture the bar moves, so for those the line carries the pose. Nothing is
//! computed for a bot at rest, unless the bar says where it looks (the pointer is over the panel): then an
//! engine body is drawn each frame with its eyes turned that way.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::avatar::mausbot::motion;
use crate::avatar::{self, Painter, looks, publish};
use crate::team::roster::Row;

/// How often an engine body gets a new picture: 30 frames a second.
const OBA_FRAME: Duration = Duration::from_micros(33_333);

/// The slot the bar draws an avatar in, in pixels, when `animate` names none: by it `motion::frame_ms`
/// says how often a pose needs to change.
pub const DEFAULT_SLOT_PX: f64 = 22.0;

/// How long a cursor or costume with `face` waits between poses in a slot of `slot` pixels, `None` when
/// it is too small to see move.
fn pose_wait(face: &str, slot: f64) -> Option<Duration> {
    let ms = motion::frame_ms(face, slot);
    (ms > 0.0).then(|| Duration::from_secs_f64(ms / 1000.0))
}

/// Frames older than this that no row shows are deleted.
pub const FRAME_GRACE: Duration = Duration::from_secs(5);

pub struct Frames {
    pub line: String,
    /// How many bots the line moves.
    pub count: usize,
    /// The pictures this line points at.
    pub paths: HashSet<PathBuf>,
}

/// The line that tells the bar nothing moves any more.
pub fn clear_line() -> String {
    json!({"v": 1, "frames": {}}).to_string()
}

pub(crate) fn key(row: &Row) -> String {
    row.id.as_str().map_or_else(|| row.id.to_string(), str::to_string)
}

fn engine_body_moves(row: &Row) -> Option<bool> {
    let look = row.look.as_ref()?;
    match avatar::painter(&look.body) {
        Painter::Oba(_) => Some(looks::shown(&look.expression, look.paint.cheering, Some(look.paint.working)).live),
        Painter::Festival | Painter::Cursor => None,
    }
}

/// Where each bot looks, by bot id: each axis -1..1, x right and y down.
pub type Gazes = HashMap<String, [f64; 2]>;

fn looks_somewhere(row: &Row, gazes: &Gazes) -> bool {
    engine_body_moves(row).is_some() && gazes.contains_key(&key(row))
}

/// How long to wait between frames: the shortest wait any moving bot needs in a slot of `slot` pixels, or
/// `None` when nothing moves. An engine body that looks somewhere needs a frame even at rest.
pub fn period(rows: &[Row], slot: f64, gazes: &Gazes) -> Option<Duration> {
    rows.iter()
        .filter_map(|row| match engine_body_moves(row) {
            Some(true) => Some(OBA_FRAME),
            Some(false) if looks_somewhere(row, gazes) => Some(OBA_FRAME),
            Some(false) => None,
            None => pose_wait(&row.look.as_ref()?.expression, slot),
        })
        .min()
}

/// The pictures drawn for bodies at rest that look somewhere, by bot id: the gaze they were drawn for
/// and the picture. A body at rest does not change while its gaze holds, so it is not drawn again.
pub type Held = HashMap<String, ([f64; 2], String)>;

/// The frames of every bot that moves, or looks somewhere, at `now_ms`, for a slot of `slot` pixels.
/// `held` keeps what a body at rest was last drawn as; the caller empties it when the bots or the slot
/// change.
pub fn render(rows: &[Row], now_ms: f64, slot: f64, gazes: &Gazes, held: &mut Held) -> Frames {
    let mut entries = Map::new();
    let mut paths = HashSet::new();
    for row in rows {
        let Some(look) = row.look.as_ref() else {
            continue;
        };
        let age_ms = (now_ms - row.since_ms).max(0.0);
        match avatar::painter(&look.body) {
            Painter::Oba(definition) => {
                let gaze = gazes.get(&key(row)).copied();
                let at_rest = engine_body_moves(row) == Some(false);
                let kept = gaze.filter(|_| at_rest).and_then(|g| held.get(&key(row)).filter(|(was, _)| *was == g));
                let url = match kept {
                    Some((_, url)) => url.clone(),
                    None => {
                        let Some(svg) = avatar::oba_frame_svg(definition, look, age_ms / 1000.0, gaze) else {
                            continue;
                        };
                        let url = publish::publish(&svg);
                        match gaze.filter(|_| at_rest && !url.is_empty()) {
                            Some(g) => held.insert(key(row), (g, url.clone())),
                            None => held.remove(&key(row)),
                        };
                        url
                    }
                };
                if url.is_empty() {
                    continue;
                }
                paths.insert(PathBuf::from(url.trim_start_matches("file://")));
                entries.insert(key(row), json!({ "avatar": url }));
            }
            Painter::Festival | Painter::Cursor => {
                if pose_wait(&look.expression, slot).is_some() {
                    entries.insert(key(row), json!({ "pose": motion::frame(&look.expression, age_ms) }));
                }
            }
        }
    }
    let count = entries.len();
    Frames { line: json!({"v": 1, "frames": Value::Object(entries)}).to_string(), count, paths }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bigger_slot_shortens_the_wait_like_frame_ms() {
        for face in ["working", "thinking", "happy"] {
            let small = pose_wait(face, DEFAULT_SLOT_PX);
            let big = pose_wait(face, DEFAULT_SLOT_PX * 2.0);
            let want = motion::frame_ms(face, DEFAULT_SLOT_PX * 2.0);
            if want > 0.0 {
                assert_eq!(big, Some(Duration::from_secs_f64(want / 1000.0)), "{face}");
                if let Some(small) = small {
                    assert!(big.expect("big") <= small, "{face}: {big:?} vs {small:?}");
                }
            } else {
                assert_eq!(big, None, "{face}");
            }
        }
        // A face that is too small to move at 22 px starts moving when the slot grows.
        let wakes = ["working", "thinking", "happy", "sleepy", "idle", "love", "error"]
            .iter()
            .any(|face| pose_wait(face, 8.0).is_none() && pose_wait(face, 66.0).is_some());
        assert!(wakes, "some face is still at 8 px and moves at 66 px");
    }
}

#[cfg(test)]
pub(crate) mod gaze_tests {
    use super::*;

    pub(crate) fn row(expression: &str) -> Row {
        let bot = json!({"mascotBody": "obj-lightbulb", "color": "blue"});
        let look = crate::avatar::mausbot::mascot::bot_look(&bot, "idle", None).expect("look");
        Row {
            id: json!("bulb-1"),
            name: json!("Bulb"),
            role: "specialist",
            department: String::new(),
            status: "idle".into(),
            task: String::new(),
            chat: String::new(),
            avatar: String::new(),
            avatar_eyes: String::new(),
            eye_pivot: None,
            expression: expression.into(),
            body: "obj-lightbulb".into(),
            colour: "blue".into(),
            variants: Vec::new(),
            avatar_extent: None,
            ink_inset: 0.0,
            motion: None,
            look: Some(look),
            since_ms: 0.0,
        }
    }

    #[test]
    fn a_bot_at_rest_has_frames_only_while_it_looks_somewhere() {
        let rows = [row("idle")];
        let id = key(&rows[0]);
        assert_eq!(period(&rows, DEFAULT_SLOT_PX, &Gazes::new()), None);
        assert_eq!(render(&rows, 0.0, DEFAULT_SLOT_PX, &Gazes::new(), &mut Held::new()).count, 0);
        let gazes = Gazes::from([(id, [0.7, -0.2])]);
        assert_eq!(period(&rows, DEFAULT_SLOT_PX, &gazes), Some(OBA_FRAME));
        assert_eq!(render(&rows, 0.0, DEFAULT_SLOT_PX, &gazes, &mut Held::new()).count, 1);
    }

    #[test]
    fn a_body_at_rest_is_not_drawn_again_while_its_gaze_holds() {
        let rows = [row("idle")];
        let id = key(&rows[0]);
        let mut held = Held::new();
        let gazes = Gazes::from([(id.clone(), [0.7, -0.2])]);
        let first = render(&rows, 0.0, DEFAULT_SLOT_PX, &gazes, &mut held);
        assert_eq!(held[&id].0, [0.7, -0.2]);
        held.get_mut(&id).expect("held").1 = "file:///kept.svg".into();
        let again = render(&rows, 33.0, DEFAULT_SLOT_PX, &gazes, &mut held);
        assert!(again.line.contains("kept.svg"), "the held picture is reused: {}", again.line);
        let moved = render(&rows, 66.0, DEFAULT_SLOT_PX, &Gazes::from([(id, [0.1, 0.1])]), &mut held);
        assert!(!moved.line.contains("kept.svg"), "a new gaze draws it again");
        assert_eq!(first.count, 1);
    }
}
