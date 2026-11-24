//! The pictures that move: what the engine computes between two snapshots while the bar shows its
//! avatars, and sends as lines of their own (`{"v":1,"frames":{...}}`, one entry per bot that moves).
//!
//! An engine body is drawn anew each frame, 30 times a second, from the formula of its state; a cursor
//! or costume is a still picture the bar moves, so for those the line carries the pose. Nothing is
//! computed for a bot at rest.

use std::collections::HashSet;
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

fn key(row: &Row) -> String {
    row.id.as_str().map_or_else(|| row.id.to_string(), str::to_string)
}

fn engine_body_moves(row: &Row) -> Option<bool> {
    let look = row.look.as_ref()?;
    match avatar::painter(&look.body) {
        Painter::Oba(_) => Some(looks::shown(&look.expression, look.paint.cheering, Some(look.paint.working)).live),
        Painter::Festival | Painter::Cursor => None,
    }
}

/// How long to wait between frames: the shortest wait any moving bot needs in a slot of `slot` pixels, or
/// `None` when nothing moves.
pub fn period(rows: &[Row], slot: f64) -> Option<Duration> {
    rows.iter()
        .filter_map(|row| match engine_body_moves(row) {
            Some(true) => Some(OBA_FRAME),
            Some(false) => None,
            None => pose_wait(&row.look.as_ref()?.expression, slot),
        })
        .min()
}

/// The frames of every bot that moves at `now_ms`, for a slot of `slot` pixels.
pub fn render(rows: &[Row], now_ms: f64, slot: f64) -> Frames {
    let mut entries = Map::new();
    let mut paths = HashSet::new();
    for row in rows {
        let Some(look) = row.look.as_ref() else {
            continue;
        };
        let age_ms = (now_ms - row.since_ms).max(0.0);
        match avatar::painter(&look.body) {
            Painter::Oba(definition) => {
                let Some(svg) = avatar::oba_frame_svg(definition, look, age_ms / 1000.0) else {
                    continue;
                };
                let url = publish::publish(&svg);
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
