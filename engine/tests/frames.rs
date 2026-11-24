//! The engine's moving avatars end to end: the daemon is told the bar wants them (`animate`), prints a line
//! of frames for the bots that move, and a line that says none does when the bar stops asking (`still`).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const ENGINE: &str = env!("CARGO_BIN_EXE_engine");

/// Clock ticks a second in /proc/<pid>/stat (`getconf CLK_TCK`; 100 on Linux).
const CLK_TCK: f64 = 100.0;

struct Daemon {
    root: PathBuf,
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Value>,
}

impl Daemon {
    fn start(name: &str, bots: &[Value]) -> Self {
        Self::start_busy(name, bots, &[])
    }

    /// Starts with a running assignment for each id in `busy`: the app draws a bot moving only while it works.
    fn start_busy(name: &str, bots: &[Value], busy: &[&str]) -> Self {
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos());
        let root = std::env::temp_dir().join(format!("omb-frames-{name}-{}-{nanos}", std::process::id()));
        for directory in ["home/usage", "rt/xdg", "data", "config", "bin"] {
            std::fs::create_dir_all(root.join(directory)).expect("scratch directory");
        }
        std::fs::write(root.join("home/bots.json"), serde_json::to_string(bots).expect("bots")).expect("bots");
        if !busy.is_empty() {
            let writer = rusqlite::Connection::open(root.join("home/runtime.db")).expect("runtime.db");
            writer.execute_batch(include_str!("schema.sql")).expect("schema");
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |at| at.as_millis());
            for bot in busy {
                writer
                    .execute(
                        "INSERT INTO assignments (assignee_bot_id, status, json, created_at)
                         VALUES (?1, 'running', '{}', ?2)",
                        rusqlite::params![bot, i64::try_from(now).unwrap_or(0)],
                    )
                    .expect("row");
            }
        }
        let hyprctl = root.join("bin/hyprctl");
        std::fs::write(&hyprctl, "#!/bin/sh\nexit 1\n").expect("hyprctl stand-in");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hyprctl, std::fs::Permissions::from_mode(0o700)).expect("mode");
        }
        let mut child = Command::new(ENGINE)
            .arg("--no-notify")
            .env("PATH", format!("{}:{}", root.join("bin").display(), std::env::var("PATH").unwrap_or_default()))
            .env("OPENMAUSBOT_HOME", root.join("home"))
            .env("XDG_RUNTIME_DIR", root.join("rt/xdg"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("OPENMAUSBOT_LOG", root.join("server.log"))
            .env("TZ", "America/Santiago")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the engine starts");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && sender.send(value).is_err()
                {
                    return;
                }
            }
        });
        Daemon { root, child, stdin, lines }
    }

    fn order(&mut self, order: &str) {
        writeln!(self.stdin, "{order}").expect("order");
        self.stdin.flush().expect("flush");
    }

    /// Everything printed in the next `span`, in order.
    fn collect(&self, span: Duration) -> Vec<Value> {
        let until = Instant::now() + span;
        let mut seen = Vec::new();
        while let Some(wait) = until.checked_duration_since(Instant::now()) {
            if let Ok(line) = self.lines.recv_timeout(wait) {
                seen.push(line);
            }
        }
        seen
    }

    fn pictures(&self) -> usize {
        std::fs::read_dir(self.root.join("rt/xdg/mission/team-avatars")).map_or(0, |entries| entries.count())
    }

    /// CPU time the daemon has used so far, in clock ticks (`utime + stime` of /proc/<pid>/stat).
    fn cpu_ticks(&self) -> u64 {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", self.child.id())).expect("stat");
        let after_name = &stat[stat.rfind(')').expect("stat has a name") + 2..];
        let fields: Vec<&str> = after_name.split(' ').collect();
        fields[11].parse::<u64>().expect("utime") + fields[12].parse::<u64>().expect("stime")
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn bot(id: &str, body: &str, expression: &str) -> Value {
    json!({"id": id, "name": id, "mascotBody": body, "mascotExpression": expression, "color": "blue"})
}

fn frames_of(lines: &[Value]) -> Vec<&Value> {
    lines.iter().filter_map(|line| line.get("frames")).collect()
}

#[test]
fn nothing_moves_until_the_bar_asks_and_the_snapshot_carries_the_still_picture() {
    let daemon = Daemon::start("quiet", &[bot("bulb", "obj-lightbulb", "working"), bot("dev", "cursor", "idle")]);
    let lines = daemon.collect(Duration::from_millis(1500));
    assert!(frames_of(&lines).is_empty(), "no frames before `animate`: {lines:?}");
    let snapshot = lines.iter().find(|line| line.get("bots").is_some()).expect("a snapshot");
    let bulb = &snapshot["bots"][0];
    assert_eq!(bulb["id"], "bulb");
    assert!(bulb["avatar"].as_str().is_some_and(|url| url.starts_with("file://")), "the still picture: {bulb}");
    assert_eq!(bulb["avatarExtent"].as_array().map(Vec::len), Some(3), "where the body sits: {bulb}");
    assert!(bulb.get("motion").is_none(), "an engine body moves itself: {bulb}");
    assert!(bulb["inkInset"].as_f64().is_some_and(|inset| inset > 0.0), "{bulb}");
    let dev = &snapshot["bots"][1];
    assert_eq!(dev["inkInset"], 3.0, "the cursor's ink starts 3 units in");
    assert!(dev["motion"]["dwell"].is_array() && dev["motion"]["blinkMs"] == 320.0, "{dev}");
    assert!(dev.get("avatarExtent").is_none());
}

#[test]
fn an_active_engine_body_gets_30_frames_a_second_and_the_cursor_a_pose_until_the_bar_says_still() {
    let mut daemon = Daemon::start_busy(
        "moving",
        &[bot("bulb", "obj-lightbulb", "working"), bot("dev", "cursor", "working")],
        &["bulb", "dev"],
    );
    daemon.collect(Duration::from_millis(800));
    daemon.order("animate");
    let lines = daemon.collect(Duration::from_millis(2000));
    let frames = frames_of(&lines);
    assert!(frames.len() >= 20, "expected about 30 frames a second, got {} in 2 s", frames.len());
    assert!(frames.len() <= 140, "the cursor's 60 Hz is the ceiling, got {} in 2 s", frames.len());
    let urls: std::collections::HashSet<&str> =
        frames.iter().filter_map(|entry| entry["bulb"]["avatar"].as_str()).collect();
    assert!(urls.len() >= 10, "frames should differ, got {} distinct pictures", urls.len());
    for url in &urls {
        assert!(url.starts_with("file://") && PathBuf::from(url.trim_start_matches("file://")).exists(), "{url}");
    }
    let pose = frames.iter().find_map(|entry| entry["dev"]["pose"].as_object()).expect("a pose for the cursor");
    for field in ["dx", "dy", "rot", "u", "sx", "sy"] {
        assert!(pose[field].is_f64(), "pose.{field}: {pose:?}");
    }
    assert!(frames.iter().all(|entry| entry["bulb"].get("pose").is_none()), "an engine body has no pose");

    daemon.order("still");
    let after = daemon.collect(Duration::from_millis(1500));
    let after_frames = frames_of(&after);
    assert!(after_frames.iter().any(|entry| entry.as_object().is_some_and(|map| map.is_empty())), "a clear line");
    let last_clear = after_frames.iter().rposition(|entry| entry.as_object().is_some_and(|map| map.is_empty()));
    assert_eq!(last_clear, Some(after_frames.len() - 1), "nothing moves after the clear line");
}

/// How many frames the cursor gets in 1.5 s after `order`.
fn cursor_frames_after(daemon: &mut Daemon, order: &str) -> usize {
    daemon.order("still");
    daemon.collect(Duration::from_millis(300));
    daemon.order(order);
    let lines = daemon.collect(Duration::from_millis(1500));
    frames_of(&lines).iter().filter(|entry| entry.get("dev").is_some()).count()
}

#[test]
fn animate_takes_the_slot_size_and_a_bigger_slot_poses_the_cursor_more_often() {
    let mut daemon = Daemon::start("slot", &[bot("dev", "cursor", "working")]);
    daemon.collect(Duration::from_millis(800));
    // `frame_ms("working", px)`: 50 ms at 10 px, 16.7 ms at 22 px and above.
    let small = cursor_frames_after(&mut daemon, "animate 10");
    let big = cursor_frames_after(&mut daemon, "animate 44");
    assert!(small >= 15, "10 px still poses the cursor, got {small}");
    assert!(big > small + 20, "a bigger slot poses it more often: {big} vs {small}");
}

#[test]
fn a_face_too_small_to_move_in_22_px_moves_in_a_bigger_slot() {
    let mut daemon = Daemon::start("slot-thinking", &[bot("dev", "cursor", "thinking")]);
    daemon.collect(Duration::from_millis(800));
    // `frame_ms("thinking", px)` is 0 up to 16 px and 50 ms from 22 px.
    assert_eq!(cursor_frames_after(&mut daemon, "animate 12"), 0, "too small to move");
    assert!(cursor_frames_after(&mut daemon, "animate 44") >= 10, "moves at 44 px");
}

#[test]
fn animate_without_a_number_or_with_a_bad_one_falls_back_to_22_px() {
    let mut daemon = Daemon::start("slot-fallback", &[bot("dev", "cursor", "working")]);
    daemon.collect(Duration::from_millis(800));
    let at_22 = cursor_frames_after(&mut daemon, "animate 22");
    for order in ["animate", "animate nope", "animate -5", "animate 0", "animate NaN"] {
        let got = cursor_frames_after(&mut daemon, order);
        assert!(got.abs_diff(at_22) <= 6, "`{order}` should behave as 22 px: {got} vs {at_22}");
    }
}

#[test]
fn a_bot_at_rest_costs_no_frames_even_while_the_bar_asks_for_them() {
    let mut daemon = Daemon::start("resting", &[bot("bulb", "obj-lightbulb", "idle"), bot("dev", "cursor", "idle")]);
    daemon.order("animate");
    let lines = daemon.collect(Duration::from_millis(1500));
    assert!(frames_of(&lines).is_empty(), "idle bots do not move: {lines:?}");
}

#[test]
fn old_frames_are_pruned_while_the_animation_runs() {
    let mut daemon = Daemon::start_busy("prune", &[bot("bulb", "obj-lightbulb", "working")], &["bulb"]);
    daemon.collect(Duration::from_millis(500));
    daemon.order("animate");
    daemon.collect(Duration::from_secs(9));
    let during = daemon.pictures();
    assert!(during < 9 * 30, "frames older than the grace must be gone, found {during} pictures after 9 s");
}

#[test]
fn an_idle_bot_on_an_active_face_gets_no_frames_but_a_busy_one_does() {
    // Researcher rests on the `searching` face; the app's sidebar draws it still until it works
    // (Sidebar.tsx:742), so the bar sends no frames for it until an assignment runs.
    let researcher = json!({"id": "res", "name": "Researcher", "title": "Research Specialist", "mascotBody": "drop", "color": "purple"});
    let mut idle = Daemon::start("researcher-idle", std::slice::from_ref(&researcher));
    idle.collect(Duration::from_millis(500));
    idle.order("animate");
    let lines = idle.collect(Duration::from_millis(1500));
    assert!(frames_of(&lines).is_empty(), "an idle researcher moves: {lines:?}");

    let mut busy = Daemon::start_busy("researcher-busy", std::slice::from_ref(&researcher), &["res"]);
    busy.collect(Duration::from_millis(500));
    busy.order("animate");
    let lines = busy.collect(Duration::from_millis(1500));
    assert!(!frames_of(&lines).is_empty(), "a busy researcher gets frames");
}

/// `cargo test --release --test frames -- --ignored --nocapture`: the CPU the engine uses with three bots
/// active at once, over 10 s. The debug build the other tests run on is several times slower.
#[test]
#[ignore = "a measurement, not a check; run it on a release build"]
fn cpu_with_three_engine_bots_active_for_ten_seconds() {
    let bots =
        [bot("a", "obj-lightbulb", "working"), bot("b", "star", "celebrate"), bot("c", "obj-lightbulb", "thinking")];
    let mut daemon = Daemon::start_busy("cpu", &bots, &["a", "b", "c"]);
    daemon.collect(Duration::from_millis(1500));
    daemon.order("animate");
    daemon.collect(Duration::from_secs(1));
    let (ticks_before, started) = (daemon.cpu_ticks(), Instant::now());
    let lines = daemon.collect(Duration::from_secs(10));
    let (ticks, wall) = (daemon.cpu_ticks() - ticks_before, started.elapsed().as_secs_f64());
    let cpu = ticks as f64 / CLK_TCK / wall * 100.0;
    let frames = frames_of(&lines).len();
    println!(
        "MEASURED engine CPU: {cpu:.2}% of one core over {wall:.1} s, {frames} frame lines ({} a second)",
        frames as f64 / wall
    );
    println!("MEASURED pictures on disk at the end: {}", daemon.pictures());
}
