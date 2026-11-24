//! What an Open Bot Avatar body shows for a bot's face, and when it celebrates.
//!
//! MausBot's bar names a bot's face with one of 39 `MausState` words (`idle`, `working`, `happy`...).
//! The engine bodies speak in `(state, mood)` instead, so `ObjectAvatar.tsx` translates with a table
//! and adds a celebration after the bot finishes: both are followed here.

use std::collections::HashMap;

use super::oba::motion::DONE_TURN_S;
use super::oba::types::{AvatarMood, AvatarState};

/// `CHEER_MS` (src/components/ObjectAvatar.tsx:33): one Done turn of the ribbon cage, and its settle.
pub const CHEER_MS: f64 = DONE_TURN_S * 1000.0 + 300.0;

/// `ACTIVE_STATES` (src/components/mascot-motion.ts:80): the faces that move while shown.
const ACTIVE_FACES: [&str; 22] = [
    "waking",
    "listening",
    "thinking",
    "searching",
    "working",
    "surprised",
    "celebrate",
    "orbit",
    "radar",
    "progress",
    "spawning",
    "loading",
    "dictating",
    "writing",
    "sending",
    "receiving",
    "uploading",
    "notifying",
    "alerting",
    "dragging",
    "bouncing",
    "powering-down",
];

/// What an engine body is drawn with: `ObjectLook` in ObjectAvatar.tsx:11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectLook {
    pub state: AvatarState,
    pub mood: AvatarMood,
}

const fn look(state: AvatarState, mood: AvatarMood) -> ObjectLook {
    ObjectLook { state, mood }
}

const IDLE: ObjectLook = look(AvatarState::Idle, AvatarMood::Neutral);
const WORKING: ObjectLook = look(AvatarState::Working, AvatarMood::Neutral);
const WAITING: ObjectLook = look(AvatarState::Waiting, AvatarMood::Neutral);
const HAPPY: ObjectLook = look(AvatarState::Idle, AvatarMood::Happy);
const DONE: ObjectLook = look(AvatarState::Done, AvatarMood::Neutral);

/// `objectLook` (ObjectAvatar.tsx:30) over the `LOOKS` table (ObjectAvatar.tsx:20-28): the face's
/// state and mood, `idle` for every face the table leaves out.
pub fn object_look(face: &str) -> ObjectLook {
    match face {
        "thinking" => look(AvatarState::Thinking, AvatarMood::Neutral),
        "working" | "writing" | "loading" | "searching" | "radar" | "orbit" | "progress" | "uploading" | "sending"
        | "receiving" => WORKING,
        "alerting" | "notifying" => WAITING,
        "sad" => look(AvatarState::Blocked, AvatarMood::Neutral),
        "celebrate" => DONE,
        "happy" | "laughing" | "proud" | "playful" => HAPPY,
        _ => IDLE,
    }
}

/// What one picture is drawn with, and whether it moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub state: AvatarState,
    pub mood: AvatarMood,
    /// `live` in ObjectAvatar.tsx:70; a picture that is not live is drawn `still`.
    pub live: bool,
}

/// `shown` in ObjectAvatar.tsx:70-71: `live = !reduced && (animated ?? (ACTIVE_STATES.has(face) ||
/// cheering))`, and a cheering bot that is live shows Done. The app's lists pass `animated` for
/// every bot (Sidebar.tsx:742: `beat || working || noticing`), so the face alone never moves an idle
/// bot there: a researcher whose resting face is `searching` is drawn still. The bar passes
/// `Some(working)` for the same reason; `None` is the app's fallback, the face's own `ACTIVE_STATES`.
/// A cheer moves any bot. The engine has no animations switch of its own (the bar owns it), so
/// motion is always on here.
pub fn shown(face: &str, cheering: bool, animated: Option<bool>) -> Shown {
    let live = animated.unwrap_or_else(|| ACTIVE_FACES.contains(&face)) || cheering;
    let ObjectLook { state, mood } = if cheering && live { DONE } else { object_look(face) };
    Shown { state, mood, live }
}

/// `useFinishCheer` (ObjectAvatar.tsx:36-50) for every bot at once: a bot cheers for `CHEER_MS` after
/// its face goes from thinking or working back to idle, and any other change of face ends it.
#[derive(Debug, Default)]
pub struct Cheers {
    seen: HashMap<String, Seen>,
}

#[derive(Debug)]
struct Seen {
    face: String,
    until: f64,
    /// When the bot took this face, in ms: the zero of its motion, as `faceStart` is in the bar.
    since: f64,
}

impl Cheers {
    /// Takes the bot's face as of `now_ms` and says whether it is cheering.
    pub fn observe(&mut self, bot: &str, face: &str, now_ms: f64) -> bool {
        let Some(seen) = self.seen.get_mut(bot) else {
            // The first face a bot is seen with is `useRef(state)`'s initial value: no cheer.
            self.seen.insert(bot.to_string(), Seen { face: face.to_string(), until: f64::NEG_INFINITY, since: now_ms });
            return false;
        };
        if seen.face != face {
            let before = object_look(&seen.face).state;
            seen.face = face.to_string();
            seen.since = now_ms;
            let finished = object_look(face).state == AvatarState::Idle
                && matches!(before, AvatarState::Working | AvatarState::Thinking);
            seen.until = if finished { now_ms + CHEER_MS } else { f64::NEG_INFINITY };
        }
        seen.until > now_ms
    }

    /// When the bot took the face it has now, in ms.
    pub fn since(&self, bot: &str) -> Option<f64> {
        self.seen.get(bot).map(|seen| seen.since)
    }

    /// When the earliest cheer still running ends, so the bar can draw the bot at rest again.
    pub fn expiry(&self, now_ms: f64) -> Option<f64> {
        self.seen.values().map(|seen| seen.until).filter(|until| *until > now_ms).min_by(f64::total_cmp)
    }

    /// Forgets bots that left the roster.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.seen.retain(|bot, _| keep(bot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 39 faces of `mascot.json`, in the app's `MausState` order.
    const FACES: [&str; 39] = [
        "sleeping",
        "waking",
        "idle",
        "listening",
        "thinking",
        "searching",
        "working",
        "excited",
        "surprised",
        "suspicious",
        "angry",
        "drowsy",
        "happy",
        "curious",
        "confused",
        "bored",
        "proud",
        "shy",
        "sad",
        "laughing",
        "scared",
        "playful",
        "celebrate",
        "orbit",
        "radar",
        "progress",
        "spawning",
        "humming",
        "loading",
        "dictating",
        "sending",
        "receiving",
        "uploading",
        "writing",
        "notifying",
        "alerting",
        "bouncing",
        "dragging",
        "powering-down",
    ];

    use AvatarMood::{Happy, Neutral};
    use AvatarState::{Blocked, Done, Idle, Thinking, Waiting, Working};

    #[test]
    fn the_39_faces_map_to_the_looks_table_of_objectavatar_tsx() {
        // Written out from ObjectAvatar.tsx:20-28, one face at a time, not derived from the code.
        let expected: [(&str, AvatarState, AvatarMood); 39] = [
            ("sleeping", Idle, Neutral),
            ("waking", Idle, Neutral),
            ("idle", Idle, Neutral),
            ("listening", Idle, Neutral),
            ("thinking", Thinking, Neutral),
            ("searching", Working, Neutral),
            ("working", Working, Neutral),
            ("excited", Idle, Neutral),
            ("surprised", Idle, Neutral),
            ("suspicious", Idle, Neutral),
            ("angry", Idle, Neutral),
            ("drowsy", Idle, Neutral),
            ("happy", Idle, Happy),
            ("curious", Idle, Neutral),
            ("confused", Idle, Neutral),
            ("bored", Idle, Neutral),
            ("proud", Idle, Happy),
            ("shy", Idle, Neutral),
            ("sad", Blocked, Neutral),
            ("laughing", Idle, Happy),
            ("scared", Idle, Neutral),
            ("playful", Idle, Happy),
            ("celebrate", Done, Neutral),
            ("orbit", Working, Neutral),
            ("radar", Working, Neutral),
            ("progress", Working, Neutral),
            ("spawning", Idle, Neutral),
            ("humming", Idle, Neutral),
            ("loading", Working, Neutral),
            ("dictating", Idle, Neutral),
            ("sending", Working, Neutral),
            ("receiving", Working, Neutral),
            ("uploading", Working, Neutral),
            ("writing", Working, Neutral),
            ("notifying", Waiting, Neutral),
            ("alerting", Waiting, Neutral),
            ("bouncing", Idle, Neutral),
            ("dragging", Idle, Neutral),
            ("powering-down", Idle, Neutral),
        ];
        assert_eq!(expected.map(|(face, ..)| face), FACES, "the test must cover every face, in order");
        for (face, state, mood) in expected {
            assert_eq!(object_look(face), ObjectLook { state, mood }, "{face}");
        }
        assert_eq!(object_look("not-a-face"), ObjectLook { state: Idle, mood: Neutral }, "idle by default");
        assert_eq!(object_look(""), ObjectLook { state: Idle, mood: Neutral });
    }

    #[test]
    fn the_faces_of_mascot_json_are_the_39_the_table_covers() {
        let data = crate::avatar::mausbot::mascot::catalogue().expect("catalogue");
        let mut faces: Vec<&str> = data["faces"].as_object().expect("faces").keys().map(String::as_str).collect();
        let mut table = FACES.to_vec();
        faces.sort_unstable();
        table.sort_unstable();
        assert_eq!(faces, table);
    }

    #[test]
    fn a_picture_moves_only_for_an_active_face_or_a_cheer() {
        let active: Vec<&str> = FACES.iter().copied().filter(|face| shown(face, false, None).live).collect();
        assert_eq!(active.len(), 22, "ACTIVE_STATES has 22 faces: {active:?}");
        for face in ["idle", "sleeping", "happy", "sad", "excited", "bored", "humming"] {
            assert!(!shown(face, false, None).live, "{face} holds still");
        }
        for face in FACES {
            assert_eq!(shown(face, false, None).live, ACTIVE_FACES.contains(&face), "{face}");
            assert!(shown(face, true, None).live, "a cheering {face} moves");
        }
    }

    #[test]
    fn an_idle_bot_with_an_active_resting_face_holds_still_like_the_sidebar() {
        // Sidebar.tsx:742 passes `animated={beat || working || noticing}`: a researcher resting on
        // `searching` is drawn still, and it moves only once it works or cheers. This was the spin the
        // bar showed on Researcher while the app held it still.
        let resting = shown("searching", false, Some(false));
        assert_eq!((resting.state, resting.live), (Working, false), "the pose is Working, drawn still");
        assert!(shown("searching", false, Some(true)).live, "a busy bot moves");
        assert!(shown("searching", true, Some(false)).live, "a cheer moves it");
        assert!(
            !shown("working", false, Some(false)).live,
            "even the working face holds still when the bot is not busy"
        );
    }

    #[test]
    fn a_cheering_bot_shows_done_whatever_its_face() {
        let calm = shown("idle", true, None);
        assert_eq!((calm.state, calm.mood, calm.live), (Done, Neutral, true));
        let happy = shown("happy", true, None);
        assert_eq!((happy.state, happy.mood), (Done, Neutral), "Done replaces the happy mood too");
        let resting = shown("happy", false, None);
        assert_eq!((resting.state, resting.mood, resting.live), (Idle, Happy, false));
    }

    #[test]
    fn the_cheer_lasts_one_done_turn_and_its_settle_4170_ms() {
        assert_eq!(CHEER_MS, 4170.0);
        assert!((DONE_TURN_S - 3.87).abs() < 1e-9);
    }

    #[test]
    fn a_bot_cheers_when_it_goes_from_working_or_thinking_back_to_idle() {
        let mut cheers = Cheers::default();
        assert!(!cheers.observe("a", "working", 1000.0), "the first face seen is not a transition");
        assert!(!cheers.observe("a", "working", 2000.0));
        assert!(cheers.observe("a", "idle", 3000.0), "working to idle cheers");
        assert!(cheers.observe("a", "idle", 3000.0 + 4169.0), "still cheering at 4169 ms");
        assert!(!cheers.observe("a", "idle", 3000.0 + 4170.0), "over at 4170 ms");
        assert!(!cheers.observe("a", "idle", 20_000.0), "and not again while it stays idle");

        let mut thinking = Cheers::default();
        thinking.observe("b", "thinking", 0.0);
        assert!(thinking.observe("b", "idle", 10.0), "thinking to idle cheers too");
    }

    #[test]
    fn only_a_finish_cheers() {
        let mut cheers = Cheers::default();
        cheers.observe("a", "idle", 0.0);
        assert!(!cheers.observe("a", "sad", 100.0), "idle to a blocked bot is no finish");
        assert!(!cheers.observe("a", "idle", 200.0), "blocked to idle is not from working or thinking");
        cheers.observe("a", "working", 300.0);
        assert!(!cheers.observe("a", "sad", 400.0), "working to blocked is not idle");
        cheers.observe("a", "waiting-room", 500.0);
        cheers.observe("a", "alerting", 600.0);
        assert!(!cheers.observe("a", "idle", 700.0), "waiting to idle is not a finish");
        cheers.observe("a", "searching", 800.0);
        assert!(cheers.observe("a", "happy", 900.0), "working to a happy idle is a finish");
    }

    #[test]
    fn another_change_of_face_ends_the_cheer_and_bots_are_independent() {
        let mut cheers = Cheers::default();
        cheers.observe("a", "working", 0.0);
        cheers.observe("b", "working", 0.0);
        assert!(cheers.observe("a", "idle", 1000.0));
        assert!(!cheers.observe("b", "working", 1000.0), "b is untouched by a's finish");
        assert!(!cheers.observe("a", "excited", 2000.0), "a new face ends the cheer");
        assert!(!cheers.observe("a", "excited", 2001.0));
    }

    #[test]
    fn the_bar_is_told_when_the_cheer_ends_and_forgets_bots_that_left() {
        let mut cheers = Cheers::default();
        assert_eq!(cheers.expiry(0.0), None);
        cheers.observe("a", "working", 0.0);
        cheers.observe("a", "idle", 1000.0);
        assert_eq!(cheers.expiry(1000.0), Some(1000.0 + 4170.0));
        assert_eq!(cheers.expiry(5170.0), None);
        cheers.retain(|bot| bot != "a");
        assert!(!cheers.observe("a", "idle", 1200.0), "a forgotten bot starts over");
    }
}
