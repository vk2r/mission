use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::{Value, json};

const ENGINE: &str = env!("CARGO_BIN_EXE_engine");

struct Scratch {
    root: PathBuf,
    fixture: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let root = std::env::temp_dir().join(format!("omb-daemon-{name}-{}-{nanos}", std::process::id()));
        for directory in ["home/usage", "rt", "rt/xdg", "data/applications", "config", "bin", "hypr"] {
            std::fs::create_dir_all(root.join(directory)).expect("scratch directory");
        }
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hypr");

        let hyprctl = root.join("bin/hyprctl");
        std::fs::write(
            &hyprctl,
            "#!/bin/sh\ncase \"$1\" in\n  clients) cat \"$HYPR_DIR/clients.json\" ;;\n  monitors) cat \"$HYPR_DIR/monitors.json\" ;;\n  dispatch) printf '%s\\n' \"$2\" >> \"$HYPR_LOG\" ;;\n  *) exit 1 ;;\nesac\n",
        )
        .expect("hyprctl stand-in");
        let notify = root.join("bin/notify-send");
        std::fs::write(&notify, "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$NOTIFY_LOG\"\n").expect("notify stand-in");
        for script in [&hyprctl, &notify] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o700)).expect("mode");
        }
        std::fs::write(
            root.join("data/applications/openmausbot.desktop"),
            "[Desktop Entry]\nType=Application\nName=OpenMausBot (fixture)\nExec=/bin/true %U\nIcon=openmausbot-fixture-icon\n",
        )
        .expect("desktop fixture");

        let scratch = Scratch { root, fixture };
        scratch.window("visible");
        scratch.bots(&[json!({"id": "boss", "name": "Boss", "chiefOfStaff": true}), json!({"id": "dev", "name": "Dev"})]);
        scratch
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn runtime(&self) -> PathBuf {
        self.root.join("rt/xdg/mission")
    }

    fn window(&self, case: &str) {
        for name in ["clients.json", "monitors.json"] {
            std::fs::copy(self.fixture.join(case).join(name), self.root.join("hypr").join(name)).expect("fixture copy");
        }
    }

    fn bots(&self, bots: &[Value]) {
        std::fs::write(self.root.join("home/bots.json"), serde_json::to_string(bots).unwrap()).expect("bots");
    }

    fn usage(&self, lines: &[String]) {
        self.usage_file(&month_file_name(today()), lines);
    }

    fn usage_file(&self, name: &str, lines: &[String]) {
        std::fs::write(self.root.join("home/usage").join(name), lines.join("\n") + "\n").expect("usage");
    }

    fn writer(&self) -> Connection {
        let connection = Connection::open(self.home().join("runtime.db")).expect("writer");
        connection.execute_batch("PRAGMA journal_mode=WAL;").expect("wal");
        connection.execute_batch("PRAGMA wal_autocheckpoint=0;").expect("autocheckpoint");
        connection.execute_batch(include_str!("schema.sql")).expect("schema");
        connection
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(ENGINE);
        command
            .args(args)
            .env("PATH", format!("{}:{}", self.root.join("bin").display(), std::env::var("PATH").unwrap_or_default()))
            .env("OPENMAUSBOT_HOME", self.home())
            .env("XDG_RUNTIME_DIR", self.root.join("rt/xdg"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("OPENMAUSBOT_LOG", self.root.join("server.log"))
            .env("HYPR_DIR", self.root.join("hypr"))
            .env("HYPR_LOG", self.root.join("hypr.log"))
            .env("NOTIFY_LOG", self.root.join("notify.log"))
            .env("TZ", "America/Santiago")
            .env_remove("OPENMAUSBOT_SETTLED_GRACE_MS")
            .env_remove("OPENMAUSBOT_LOG_POLL");
        command
    }

    fn run_json(&self, args: &[&str]) -> Value {
        self.run_json_in(args, &[])
    }

    fn run_json_in(&self, args: &[&str], env: &[(&str, &str)]) -> Value {
        let mut command = self.command(args);
        for (key, value) in env {
            command.env(key, value);
        }
        let output = command.output().expect("the engine runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(stdout.trim())
            .unwrap_or_else(|error| panic!("{args:?} did not print JSON ({error}): {stdout}"))
    }

    fn log(&self, line: &str) {
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("server.log"))
            .expect("log");
        log.write_all(line.as_bytes()).expect("log line");
        log.write_all(b"\n").expect("newline");
    }

    fn pops(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("notify.log")).map(|text| text.lines().map(str::to_string).collect()).unwrap_or_default()
    }

    fn dispatches(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("hypr.log")).map(|text| text.lines().map(str::to_string).collect()).unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Daemon {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Daemon {
    fn start(scratch: &Scratch, extra: &[(&str, &str)]) -> Self {
        let mut command = scratch.command(&[]);
        for (key, value) in extra {
            command.env(key, value);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the daemon starts");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout");
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if sender.send(line).is_err() {
                    return;
                }
            }
        });
        Daemon { child, stdin, lines }
    }

    fn next(&self, timeout: Duration) -> Option<Value> {
        self.lines.recv_timeout(timeout).ok().map(|line| {
            serde_json::from_str(&line).unwrap_or_else(|error| panic!("not JSON ({error}): {line}"))
        })
    }

    fn until(&self, field: &str, value: Value, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let line = self.next(left)?;
            if line[field] == value {
                return Some(line);
            }
        }
        None
    }

    fn until_cost(&self, cost: f64, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let line = self.next(left)?;
            if line["costs"]["day"]["costUsd"] == json!(cost) {
                return Some(line);
            }
        }
        None
    }

    fn until_status(&self, status: &str, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let line = self.next(left)?;
            if line["bots"][1]["status"] == json!(status) {
                return Some(line);
            }
        }
        None
    }

    fn write(&mut self, order: &str) {
        let stdin = self.stdin.as_mut().expect("stdin is open");
        stdin.write_all(order.as_bytes()).expect("write");
        stdin.write_all(b"\n").expect("write");
        stdin.flush().expect("flush");
    }

    fn close_stdin(&mut self) {
        self.stdin = None;
    }

    fn wait(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait().expect("wait") {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn today() -> jiff::civil::Date {
    let zone = jiff::tz::TimeZone::get("America/Santiago").expect("zone");
    jiff::Timestamp::now().to_zoned(zone).date()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

#[test]
fn the_first_line_is_quick_and_an_idle_daemon_says_nothing() {
    let scratch = Scratch::new("first");
    scratch.writer();
    let mut daemon = Daemon::start(&scratch, &[]);
    let started = Instant::now();

    let first = daemon.next(Duration::from_millis(300)).expect("the first line under 300 ms");
    assert_eq!(first["v"], 1);
    assert_eq!(first["ok"], true);
    assert_eq!(first["window"]["state"], "visible");
    assert!(first.get("activity").is_none(), "the Activity list is gone (step 15)");
    assert!(started.elapsed() < Duration::from_millis(300), "first line took {:?}", started.elapsed());

    assert!(daemon.next(Duration::from_secs(3)).is_none(), "an idle tick must not say anything");
    assert!(daemon.wait(Duration::from_millis(50)).is_none(), "the daemon keeps running");
}

#[test]
fn each_source_is_news_within_a_second_and_a_half() {
    let scratch = Scratch::new("changes");
    let writer = scratch.writer();
    let daemon = Daemon::start(&scratch, &[]);
    assert!(daemon.next(Duration::from_millis(300)).is_some(), "first line");

    let sent = Instant::now();
    writer
        .execute(
            "INSERT INTO assignments (assignee_bot_id, status, json, created_at)
             VALUES ('dev', 'running', '{\"origin\": {\"quote\": \"ship step six\"}}', ?1)",
            [now_ms()],
        )
        .expect("row");
    let line = daemon.until("busy", json!(true), Duration::from_millis(1500)).expect("a committed row within 1,5 s");
    let took = sent.elapsed();
    assert!(line["working"] == json!(1));
    assert!(took < Duration::from_millis(1500), "a committed row took {took:?}");
    assert_eq!(line["bots"][1]["task"], "ship step six");

    let sent = Instant::now();
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(scratch.root.join("server.log"))
        .expect("log");
    log.write_all(b"[2026-11-01T01:10:00.000Z] [out] [omb-turn] bot=dev text=\"Step six done\" depth=1\n")
        .expect("log line");
    drop(log);
    let line = daemon.until("alert", json!(true), Duration::from_millis(1500)).expect("a turn within 1,5 s");
    let took = sent.elapsed();
    assert!(took < Duration::from_millis(1500), "a turn took {took:?}");
    assert_eq!(line["alert"], true);
    assert_eq!(scratch.pops(), ["-a", "MausBot", "-i", "openmausbot-fixture-icon", "--", "MausBot", "Step six done"]);
    assert!(scratch.runtime().join("alert").exists(), "the alert is stamped before the popup");

    let sent = Instant::now();
    scratch.usage(&[spend(&today(), "Dev", 1.25)]);
    daemon.until_cost(1.25, Duration::from_millis(1500)).expect("a spend record within 1,5 s");
    let took = sent.elapsed();
    assert!(took < Duration::from_millis(1500), "a spend record took {took:?}");

    scratch.bots(&[
        json!({"id": "boss", "name": "Boss", "chiefOfStaff": true}),
        json!({"id": "dev", "name": "Dev"}),
        json!({"id": "ops", "name": "Ops"}),
    ]);
    let line = daemon.next(Duration::from_millis(1500)).expect("a new bot within 1,5 s");
    assert_eq!(line["bots"].as_array().expect("rows").len(), 3);

    assert!(daemon.next(Duration::from_secs(2)).is_none());
}

#[test]
fn a_turn_still_raises_the_alert_and_the_line_carries_no_activity_list() {
    let scratch = Scratch::new("turn-alert");
    scratch.writer();
    let daemon = Daemon::start(&scratch, &[]);
    let first = daemon.next(Duration::from_millis(300)).expect("first line");
    assert!(first.get("activity").is_none(), "the Activity list is gone (step 15)");

    let sent = Instant::now();
    scratch.log("[2026-11-01T01:10:00.000Z] [out] [omb-turn] bot=dev text=\"Step nine\" images=0 depth=1 card=false");
    let line = daemon.until("alert", json!(true), Duration::from_millis(1500)).expect("a turn within 1,5 s");
    let took = sent.elapsed();
    assert!(took < Duration::from_millis(1500), "the turn took {took:?} to reach the bar");
    assert!(line.get("activity").is_none(), "the line no longer carries a turn list");

    scratch.log("[2026-11-01T01:10:01.000Z] [out] [omb-turn] bot=boss text=\"my own message\" images=0 depth=0 card=false");
    assert!(daemon.next(Duration::from_millis(1500)).is_none(), "a depth=0 line is not news");
}

#[test]
fn a_restarted_engine_does_not_announce_the_turns_it_already_saw() {
    let scratch = Scratch::new("restart-replay");
    scratch.writer();
    scratch.log("[2026-11-01T01:10:00.000Z] [out] [omb-turn] bot=dev text=\"History\" depth=1");

    let daemon = Daemon::start(&scratch, &[]);
    let first = daemon.next(Duration::from_millis(300)).expect("first line");
    assert_eq!(first["alert"], json!(false), "the log is older than this run");
    scratch.log("[2026-11-01T01:10:05.000Z] [out] [omb-turn] bot=dev text=\"During the run\" depth=1");
    daemon.until("alert", json!(true), Duration::from_millis(1500)).expect("the new turn");
    drop(daemon);

    let restarted = Daemon::start(&scratch, &[]);
    assert!(restarted.next(Duration::from_millis(300)).is_some(), "first line");
    assert!(restarted.next(Duration::from_secs(2)).is_none(), "a restart must not replay the log");
}

#[test]
fn the_window_is_followed_even_when_nothing_asks_for_it() {
    let scratch = Scratch::new("window");
    scratch.writer();
    let daemon = Daemon::start(&scratch, &[]);
    assert_eq!(daemon.next(Duration::from_millis(300)).expect("first line")["window"]["state"], "visible");

    scratch.window("parked-hidden");
    let line = daemon.until("window", json!({"state": "hidden", "address": "0x1000003", "running": true, "launching": false, "installed": true}), Duration::from_secs(3))
        .expect("the window is looked at again within two ticks");
    assert_eq!(line["window"]["state"], "hidden");
}

#[test]
fn an_order_is_sent_to_hyprland_and_an_unknown_one_is_ignored() {
    let scratch = Scratch::new("orders");
    scratch.writer();
    let mut daemon = Daemon::start(&scratch, &[]);
    assert!(daemon.next(Duration::from_millis(300)).is_some(), "first line");

    daemon.write("please do something");
    daemon.write("hide");
    let deadline = Instant::now() + Duration::from_secs(2);
    while scratch.dispatches().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let sent = scratch.dispatches();
    assert_eq!(sent.len(), 1, "one order, one dispatch: {sent:?}");
    assert!(sent[0].contains("hl.dsp.window.move"), "{}", sent[0]);
    assert!(daemon.wait(Duration::from_millis(50)).is_none(), "the daemon keeps running");
}

#[test]
fn now_shows_the_five_minutes_running_out_without_waiting_for_it() {
    let scratch = Scratch::new("grace");
    let reference: jiff::Timestamp = "2026-11-01T12:00:00-03:00".parse().expect("timestamp");
    let writer = scratch.writer();
    let finished = reference.as_millisecond() - 6 * 60 * 1000;
    writer
        .execute(
            "INSERT INTO assignments (assignee_bot_id, status, json, created_at, finished_at)
             VALUES ('dev', 'failed', '{}', ?1, ?1)",
            [finished],
        )
        .expect("row");

    let inside: jiff::Timestamp = "2026-11-01T11:56:00-03:00".parse().expect("timestamp");
    let after: jiff::Timestamp = "2026-11-01T12:00:00-03:00".parse().expect("timestamp");
    let badge = scratch.run_json(&["--once", "--now", &inside.to_string()]);
    assert_eq!(badge["bots"][1]["status"], "failed", "four minutes after the failure the badge is still there");
    let gone = scratch.run_json(&["--once", "--now", &after.to_string()]);
    assert_eq!(gone["bots"][1]["status"], "idle", "past the five minutes it is gone, and the database never changed");
}

#[test]
fn the_daemon_itself_lets_the_badge_go_when_its_grace_ends() {
    let scratch = Scratch::new("grace-live");
    let writer = scratch.writer();
    let daemon = Daemon::start(&scratch, &[("OPENMAUSBOT_SETTLED_GRACE_MS", "2500")]);
    assert_eq!(daemon.next(Duration::from_millis(300)).expect("first line")["bots"].as_array().unwrap().len(), 2);

    writer
        .execute(
            "INSERT INTO assignments (assignee_bot_id, status, json, created_at, finished_at)
             VALUES ('dev', 'failed', '{}', ?1, ?1)",
            [now_ms()],
        )
        .expect("row");
    let badge = daemon.until_status("failed", Duration::from_millis(1500)).expect("the row is news");
    assert_eq!(badge["bots"][1]["status"], "failed");

    let after = daemon.until_status("idle", Duration::from_secs(5)).expect("the grace running out is a line");
    assert_eq!(after["bots"].as_array().unwrap().len(), 2);
}

#[test]
fn midnight_recounts_the_day() {
    let scratch = Scratch::new("midnight");
    scratch.writer();
    scratch.usage_file("2026-11.jsonl", &[
        "{\"at\":\"2026-11-01T23:59:00-03:00\",\"costUsd\":1.0,\"outcome\":{\"status\":\"ok\"}}".to_string(),
        "{\"at\":\"2026-11-02T00:00:30-03:00\",\"costUsd\":2.0,\"outcome\":{\"status\":\"ok\"}}".to_string(),
    ]);

    let late: jiff::Timestamp = "2026-11-01T23:59:30-03:00".parse().expect("timestamp");
    let early: jiff::Timestamp = "2026-11-02T00:00:40-03:00".parse().expect("timestamp");
    let before = scratch.run_json(&["--once", "--now", &late.to_string()]);
    assert_eq!(before["costs"]["day"]["to"], "2026-11-01");
    assert_eq!(before["costs"]["day"]["costUsd"], 1.0);
    let after = scratch.run_json(&["--once", "--now", &early.to_string()]);
    assert_eq!(after["costs"]["day"]["to"], "2026-11-02");
    assert_eq!(after["costs"]["day"]["costUsd"], 2.0);
    assert_eq!(after["costs"]["day"]["tasks"], 1);
}

fn spend(day: &jiff::civil::Date, bot: &str, cost: f64) -> String {
    format!(" {{\"at\":\"{day}T12:00:00\",\"botName\":\"{bot}\",\"costUsd\":{cost},\"outcome\":{{\"status\":\"ok\"}}}}").trim_start().to_string()
}

fn month_file_name(day: jiff::civil::Date) -> String {
    format!("{}-{:02}.jsonl", day.year(), day.month())
}

#[test]
fn the_three_ranges_come_in_one_line() {
    let scratch = Scratch::new("ranges");
    scratch.writer();
    let today = today();
    let yesterday = today.checked_sub(jiff::Span::new().days(1)).expect("yesterday");
    let mut files: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    files.entry(month_file_name(yesterday)).or_default().push(spend(&yesterday, "Boss", 1.0));
    files.entry(month_file_name(today)).or_default().push(spend(&today, "Dev", 2.5));
    for (name, lines) in &files {
        scratch.usage_file(name, lines);
    }

    let daemon = Daemon::start(&scratch, &[]);
    let line = daemon.next(Duration::from_millis(500)).expect("first line");
    let costs = &line["costs"];
    for range in ["day", "week", "month"] {
        assert_eq!(costs[range]["ok"], json!(true), "{range}");
        assert_eq!(costs[range]["range"], json!(range));
        assert_eq!(costs[range]["to"], json!(today.to_string()), "{range} ends today");
    }

    assert_eq!(costs["day"]["costUsd"], json!(2.5));
    let dev = &costs["day"]["byBot"][0];
    assert_eq!((&dev["name"], &dev["costUsd"], &dev["botId"]), (&json!("Dev"), &json!(2.5), &json!("Dev")));
    assert_eq!(costs["day"]["byBot"].as_array().expect("rows").len(), 1);
    assert_eq!(dev["series"].as_array().expect("series").len(), 24);

    let week_from: jiff::civil::Date = costs["week"]["from"].as_str().expect("from").parse().expect("date");
    let month_from: jiff::civil::Date = costs["month"]["from"].as_str().expect("from").parse().expect("date");
    assert_eq!(week_from, today.checked_sub(jiff::Span::new().days(6)).expect("six days back"));
    assert_eq!(month_from, today.checked_sub(jiff::Span::new().days(29)).expect("29 days back"));
    assert!(costs["week"]["costUsd"].as_f64().expect("cost") >= 2.5);
    assert!(costs["month"]["costUsd"].as_f64().expect("cost") >= 2.5);

    for (range, bars) in [("day", 24usize), ("week", 7), ("month", 30)] {
        let series = costs[range]["series"].as_array().expect("series");
        assert_eq!(series.len(), bars, "{range}");
        let sum: f64 = series.iter().map(|bar| bar["costUsd"].as_f64().expect("bar")).sum();
        let total = costs[range]["costUsd"].as_f64().expect("total");
        assert!((sum - total).abs() < 0.005, "{range}: the bars add up to the total");
    }

    scratch.usage(&[spend(&today, "Dev", 2.5), spend(&today, "Boss", 0.5)]);
    let line = daemon.next(Duration::from_millis(1500)).expect("a spend record is news");
    assert_eq!(line["costs"]["day"]["costUsd"], json!(3.0));
    let dev = &line["costs"]["day"]["byBot"][0];
    assert_eq!((&dev["name"], &dev["costUsd"], &dev["botId"]), (&json!("Dev"), &json!(2.5), &json!("Dev")));
}

#[test]
fn a_tick_without_a_usage_change_does_not_recount() {
    let scratch = Scratch::new("no-recount");
    scratch.writer();
    let today = today();
    let file = scratch.home().join("usage").join(month_file_name(today));
    let line_with = |cost: &str| format!("{{\"at\":\"{today}T12:00:00\",\"botName\":\"Dev\",\"costUsd\":{cost}}}");

    scratch.usage(&[line_with("1.25")]);
    let daemon = Daemon::start(&scratch, &[]);
    let first = daemon.next(Duration::from_millis(500)).expect("first line");
    assert_eq!(first["costs"]["day"]["costUsd"], json!(1.25));

    let reference = scratch.root.join("usage.ref");
    let copied = Command::new("cp").args(["-p", &file.display().to_string(), &reference.display().to_string()]).status().expect("cp");
    assert!(copied.success());
    std::fs::write(&file, line_with("9.25") + "\n").expect("rewrite");
    let restored = Command::new("touch").args(["-r", &reference.display().to_string(), &file.display().to_string()]).status().expect("touch");
    assert!(restored.success());

    assert!(daemon.next(Duration::from_millis(2500)).is_none(), "an unchanged usage file must not be read again");

    scratch.usage(&[line_with("1.25"), spend(&today, "Boss", 0.25)]);
    let line = daemon.next(Duration::from_millis(1500)).expect("a new record is news");
    assert_eq!(line["costs"]["day"]["costUsd"], json!(1.5));
}

#[test]
fn a_bad_now_is_a_usage_error() {
    let scratch = Scratch::new("bad-now");
    let output = scratch.command(&["--once", "--now", "midnight"]).output().expect("runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stdout).is_empty());
    let output = scratch.command(&["--nope"]).output().expect("runs");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn closing_stdin_ends_the_daemon_in_under_a_second() {
    let scratch = Scratch::new("eof");
    scratch.writer();
    let mut daemon = Daemon::start(&scratch, &[]);
    assert!(daemon.next(Duration::from_millis(300)).is_some(), "first line");

    daemon.close_stdin();
    let closed = Instant::now();
    let status = daemon.wait(Duration::from_secs(1)).expect("the daemon leaves when its pipe closes");
    assert_eq!(status.code(), Some(0));
    assert!(closed.elapsed() < Duration::from_secs(1), "it took {:?}", closed.elapsed());
}

#[test]
fn a_killed_parent_leaves_no_engine_behind() {
    let scratch = Scratch::new("orphan");
    scratch.writer();
    let mut shell = Command::new("sh");
    shell
        .arg("-c")
        .arg(format!("'{ENGINE}' > /dev/null 2>&1 & echo $!; exec sleep 60"))
        .env("PATH", format!("{}:{}", scratch.root.join("bin").display(), std::env::var("PATH").unwrap_or_default()))
        .env("OPENMAUSBOT_HOME", scratch.home())
        .env("XDG_RUNTIME_DIR", scratch.root.join("rt/xdg"))
        .env("XDG_DATA_HOME", scratch.root.join("data"))
        .env("OPENMAUSBOT_LOG", scratch.root.join("server.log"))
        .env("HYPR_DIR", scratch.root.join("hypr"))
        .env("HYPR_LOG", scratch.root.join("hypr.log"))
        .env("NOTIFY_LOG", scratch.root.join("notify.log"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut shell = shell.spawn().expect("the shell starts");
    let parent = shell.id();
    let mut stdout = BufReader::new(shell.stdout.take().expect("stdout"));
    let mut line = String::new();
    stdout.read_line(&mut line).expect("the child pid");
    let engine: i32 = line.trim().parse().expect("a pid");
    assert!(proc_exists(engine), "the engine is running");

    Command::new("kill").arg("-9").arg(parent.to_string()).status().expect("kill");
    let deadline = Instant::now() + Duration::from_secs(2);
    while proc_exists(engine) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(!proc_exists(engine), "the engine outlived its parent: pid {engine}");
    let _ = shell.wait();
}

fn proc_exists(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !stat.rsplit(')').next().unwrap_or("").trim_start().starts_with('Z'),
        Err(_) => false,
    }
}

#[test]
fn only_one_daemon_announces_and_the_other_never_exits_three() {
    let scratch = Scratch::new("lock");
    scratch.writer();
    let mut first = Daemon::start(&scratch, &[]);
    let mut second = Daemon::start(&scratch, &[]);
    assert!(first.next(Duration::from_millis(300)).is_some(), "the first daemon draws");
    assert!(second.next(Duration::from_millis(300)).is_some(), "a second shell's daemon draws too");

    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(scratch.root.join("server.log"))
        .expect("log");
    log.write_all(b"[2026-11-01T01:10:00.000Z] [out] [omb-turn] bot=dev text=\"Only once\" depth=1\n").expect("line");
    drop(log);

    assert!(first.until("alert", json!(true), Duration::from_millis(1500)).is_some());
    assert!(second.until("alert", json!(true), Duration::from_millis(1500)).is_some());
    assert!(second.wait(Duration::from_millis(200)).is_none(), "a busy lock is not exit 3");
    assert!(first.wait(Duration::from_millis(50)).is_none());
    let pops = scratch.pops();
    assert_eq!(pops.iter().filter(|arg| *arg == "Only once").count(), 1, "one popup, not two: {pops:?}");
}

#[test]
fn the_wal_stays_free_to_checkpoint_while_the_daemon_reads() {
    let scratch = Scratch::new("wal");
    let writer = scratch.writer();
    let mut daemon = Daemon::start(&scratch, &[]);
    assert!(daemon.next(Duration::from_millis(300)).is_some(), "first line");

    for turn in 0..120 {
        writer
            .execute(
                "INSERT INTO assignments (assignee_bot_id, status, json, created_at)
                 VALUES ('dev', 'running', '{}', ?1)",
                [now_ms() + turn],
            )
            .expect("the app's write");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        daemon.until("working", json!(120), Duration::from_secs(4)).is_some(),
        "the daemon read every row"
    );

    let mut busy = 1;
    let mut attempt = 0;
    while busy != 0 && attempt < 40 {
        busy = writer
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get::<_, i64>(0))
            .expect("checkpoint");
        attempt += 1;
        if busy != 0 {
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    assert_eq!(busy, 0, "the writer could not checkpoint while the daemon was reading");
    let wal = scratch.home().join("runtime.db-wal");
    let size = std::fs::metadata(&wal).map(|metadata| metadata.len()).unwrap_or(0);
    assert_eq!(size, 0, "the WAL is empty after a TRUNCATE checkpoint");
    assert!(daemon.wait(Duration::from_millis(50)).is_none(), "the daemon is still running");
}

#[test]
fn once_prints_the_whole_state_and_a_degraded_database_reads_empty() {
    let scratch = Scratch::new("once");
    let writer = scratch.writer();
    let at = now_ms();
    writer
        .execute_batch(&format!(
            "INSERT INTO assignments (assignee_bot_id, status, json, created_at, finished_at) VALUES
               ('dev', 'running', '{{\"origin\": {{\"quote\": \"ship it\"}}}}', {at}, NULL),
               ('boss', 'waiting', '{{\"brief\": \"review\"}}', {at}, NULL),
               ('dev', 'failed', '{{}}', {at}, {at});"
        ))
        .expect("rows");
    scratch.usage(&[spend(&today(), "Dev", 0.75)]);
    std::fs::create_dir_all(scratch.runtime()).expect("runtime directory");
    std::fs::write(scratch.runtime().join("alert"), "1793506216413147313\n").expect("a pending alert");

    let once = scratch.run_json(&["--once"]);
    assert_eq!(once["v"], 1);
    assert_eq!(once["ok"], true);
    assert_eq!(once["alert"], true);
    assert_eq!(once["busy"], true);
    assert_eq!(once["costs"]["day"]["costUsd"], 0.75);
    assert!(once["bots"].as_array().expect("rows").len() >= 2);
    assert!(once.get("activity").is_none(), "the line no longer carries a turn list");
    assert!(once.get("today").is_none());

    std::fs::remove_file(scratch.home().join("runtime.db")).ok();
    std::fs::remove_file(scratch.home().join("runtime.db-wal")).ok();
    std::fs::remove_file(scratch.home().join("runtime.db-shm")).ok();
    let once = scratch.run_json(&["--once"]);
    assert_eq!(once["working"], 0);
    assert_eq!(once["busy"], false);
}
