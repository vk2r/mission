use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::os::unix::fs::MetadataExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use jiff::{SignedDuration, Timestamp, civil};
use serde_json::{Value, json};

use crate::db::{Assignments, Database};
use crate::hypr::{self, Desktop, Hyprland, Monitor, Order, Window};
use crate::logwatch::{self, Emitter, Notifier, Tail};
use crate::ledger::Ledger;
use crate::{alert, avatars, launch, paths, roster, usage};

const TICK: Duration = Duration::from_secs(1);
const WINDOW_PERIOD: Duration = Duration::from_secs(2);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);
const SETTLE: Duration = Duration::from_millis(250);
const LOCK_RETRY: Duration = Duration::from_secs(5);

const PICTURE_GRACE: Duration = Duration::from_secs(65);

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub once: bool,
    pub notify: bool,
    pub now: Option<Timestamp>,
}

pub trait Clock {
    fn now(&self) -> Timestamp;
}

pub struct Real;

impl Clock for Real {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

pub struct Shifted(SignedDuration);

impl Shifted {
    pub fn to(reference: Timestamp) -> Self {
        Shifted(reference.duration_since(Timestamp::now()))
    }
}

impl Clock for Shifted {
    fn now(&self) -> Timestamp {
        Timestamp::now() + self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    mtime: i64,
    nanos: i64,
    size: u64,
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(Stamp {
        device: metadata.dev(),
        inode: metadata.ino(),
        mtime: metadata.mtime(),
        nanos: metadata.mtime_nsec(),
        size: metadata.len(),
    })
}

pub struct Engine {
    clock: Box<dyn Clock>,
    desktop: Box<dyn Desktop>,
    notify: bool,
    runtime: Option<PathBuf>,
    lock: Option<File>,
    emitter: Option<Emitter<Box<dyn Notifier>>>,
    lock_retry: Option<Instant>,

    fresh: bool,

    roster: Option<Vec<Value>>,
    roster_stamp: Option<Stamp>,
    database: Option<Database>,
    database_id: Option<(u64, u64)>,
    data_version: Option<i64>,
    assignments: Option<Assignments>,
    working: i64,
    grace: f64,
    bots: Vec<roster::Row>,
    busy: bool,
    shown: (bool, i64),
    bots_dirty: bool,
    expiry: Option<f64>,

    costs: Value,
    usage_stamps: Vec<(String, Option<Stamp>)>,
    usage_day: Option<civil::Date>,
    ledger: Ledger,
    known: Option<usage::Known>,
    kept: std::collections::HashSet<PathBuf>,
    prune_at: Option<Instant>,

    window: Value,
    window_at: Option<Instant>,
    launching: Option<Instant>,

    alert: bool,
    alert_stamps: (Option<Stamp>, Option<Stamp>),

    log: Option<PathBuf>,
    tail: Tail,

    changed: bool,
    last_line: Option<String>,
}

impl Engine {
    pub fn new(options: &Options) -> Self {
        let clock: Box<dyn Clock> = match options.now {
            Some(reference) => Box::new(Shifted::to(reference)),
            None => Box::new(Real),
        };
        let runtime = paths::runtime_dir();
        let lock = runtime.as_deref().and_then(|directory| logwatch::claim(directory).ok());
        let log = paths::log_file();
        let tail = Tail::at_end(log.as_deref().and_then(logwatch::size_of).unwrap_or(0));
        let mut engine = Engine {
            clock,
            desktop: Box::new(Hyprland),
            notify: options.notify,
            runtime,
            lock: None,
            emitter: None,
            lock_retry: None,
            fresh: true,
            roster: None,
            roster_stamp: None,
            database: None,
            database_id: None,
            data_version: None,
            assignments: None,
            working: 0,
            grace: roster::grace_ms(),
            bots: Vec::new(),
            busy: false,
            shown: (false, 0),
            bots_dirty: true,
            expiry: None,
            costs: json!({}),
            usage_stamps: Vec::new(),
            usage_day: None,
            ledger: Ledger::new(),
            known: None,
            kept: std::collections::HashSet::new(),
            prune_at: None,
            window: with_launch_flags(json!({"state": "absent", "address": ""}), None),
            window_at: None,
            launching: None,
            alert: false,
            alert_stamps: (None, None),
            log,
            tail,
            changed: true,
            last_line: None,
        };
        if !options.once {
            engine.emitter = lock.as_ref().and_then(|_| engine.announcer());
            engine.lock = lock;
        }
        engine
    }

    fn announcer(&self) -> Option<Emitter<Box<dyn Notifier>>> {
        let directory = self.runtime.clone()?;
        let notifier: Box<dyn Notifier> =
            if self.notify { Box::new(logwatch::Desktop) } else { Box::new(Silent) };
        Some(Emitter::new(directory, notifier))
    }

    pub fn tick(&mut self, force_window: bool) -> Option<String> {
        let now = self.clock.now();
        self.observe(now, force_window);
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        let line = self.line()?;
        if self.last_line.as_deref() == Some(line.as_str()) {
            return None;
        }
        self.last_line = Some(line.clone());
        Some(line)
    }

    fn order(&mut self, name: &str) {
        if name == "refresh" {
            self.fresh = true;
            return;
        }
        let Some(order) = Order::parse(name) else {
            return;
        };
        let report = hypr::run(order, self.desktop.as_ref());
        if order == Order::Launch && report.exit == 0 {
            self.launching = Some(Instant::now());
        }
        self.window_at = None;
    }

    fn observe(&mut self, now: Timestamp, force_window: bool) {
        let first = std::mem::take(&mut self.fresh);

        self.claim();
        self.follow_log();

        let mut bots_dirty = first || self.bots_dirty;

        let roster_stamp = paths::bots_file().as_deref().and_then(stamp_of);
        let mut known_changed = false;
        if first || roster_stamp != self.roster_stamp {
            self.roster = roster::read();
            self.roster_stamp = roster_stamp;
            bots_dirty = true;
            let known = self.roster.as_deref().and_then(roster::known);
            known_changed = known != self.known;
            self.known = known;
        }

        if self.refresh_assignments() {
            bots_dirty = true;
        }

        if self.expiry.is_some_and(|at| now.as_millisecond() as f64 >= at) {
            bots_dirty = true;
        }

        if self.prune_at.is_some_and(|at| Instant::now() >= at) {
            bots_dirty = true;
        }

        if bots_dirty {
            self.rebuild_bots(now);
        }

        let directory = paths::usage_dir();
        let day = now.to_zoned(usage::local_zone()).date();
        let stamps: Vec<(String, Option<Stamp>)> = usage::watched_files(now)
            .into_iter()
            .map(|name| {
                let stamp = directory.as_ref().and_then(|directory| stamp_of(&directory.join(&name)));
                (name, stamp)
            })
            .collect();
        if first || stamps != self.usage_stamps || Some(day) != self.usage_day || known_changed {
            self.costs = usage::reports_at(&mut self.ledger, directory.as_deref(), now, self.known.as_ref());
            self.changed = true;
            self.usage_stamps = stamps;
            self.usage_day = Some(day);
        }

        let stamps = match self.runtime.clone() {
            Some(directory) => (stamp_of(&directory.join("alert")), stamp_of(&directory.join("seen"))),
            None => (None, None),
        };
        if first || stamps != self.alert_stamps {
            self.alert = self.runtime.as_deref().is_some_and(alert::pending_in);
            self.alert_stamps = stamps;
            self.changed = true;
        }

        let due = force_window || self.window_at.is_none_or(|at| at.elapsed() >= WINDOW_PERIOD);
        if due {
            self.read_window();
        }
    }

    fn claim(&mut self) {
        if self.lock.is_some() {
            return;
        }
        let now = Instant::now();
        if self.lock_retry.is_some_and(|at| now < at) {
            return;
        }
        self.lock_retry = Some(now + LOCK_RETRY);
        let Some(directory) = self.runtime.clone() else {
            return;
        };
        if let Ok(lock) = logwatch::claim(&directory) {
            self.lock = Some(lock);
            self.emitter = self.announcer();
        }
    }

    fn follow_log(&mut self) {
        let Some(log) = self.log.clone() else {
            return;
        };
        let lines = logwatch::advance(&log, &mut self.tail);
        for line in &lines {
            if let Some(emitter) = &self.emitter {
                emitter.announce(line);
            }
        }
    }

    fn refresh_assignments(&mut self) -> bool {
        let Some(path) = paths::db_file() else {
            return self.close_database();
        };
        let Some(file) = stamp_of(&path) else {
            return self.close_database();
        };
        let id = (file.device, file.inode);
        if self.database.is_some() && self.database_id != Some(id) {
            self.close_database();
        }
        if self.database.is_none() {
            let Ok(database) = Database::open(&path) else {
                return self.close_database();
            };
            self.database = Some(database);
            self.database_id = Some(id);
            self.data_version = None;
        }
        let version = self.database.as_ref().and_then(|database| database.data_version().ok());
        let Some(version) = version else {
            return self.close_database();
        };
        if self.data_version == Some(version) {
            return false;
        }
        self.data_version = Some(version);

        let read = self
            .database
            .as_ref()
            .map(|database| (database.read(), database.busy_count().unwrap_or(0)));
        match read {
            Some((Ok(assignments), busy)) => {
                self.assignments = Some(assignments);
                self.working = busy;
            }
            _ => {
                let changed = self.assignments.is_some();
                self.assignments = None;
                self.working = 0;
                self.database = None;
                self.database_id = None;
                self.data_version = None;
                return changed;
            }
        }
        true
    }

    fn close_database(&mut self) -> bool {
        let had_connection = self.database.take().is_some();
        let had_rows = self.assignments.take().is_some();
        let changed = had_connection || had_rows;
        self.database_id = None;
        self.data_version = None;
        self.working = 0;
        changed
    }

    fn rebuild_bots(&mut self, now: Timestamp) {
        self.bots_dirty = false;
        let now_ms = now.as_millisecond() as f64;
        let bots = match self.roster.as_deref() {
            Some(roster) => roster::rows_for_at(roster, self.assignments.as_ref(), now_ms, self.grace),
            None => Vec::new(),
        };
        if bots != self.bots {
            self.bots = bots;
            self.changed = true;
        }
        let kept = roster::picture_paths(&self.bots);
        let due = self.prune_at.is_some_and(|at| Instant::now() >= at);
        if kept != self.kept || due {
            avatars::prune(&kept);
            self.prune_at = (kept != self.kept).then(|| Instant::now() + PICTURE_GRACE);
            self.kept = kept;
        }

        self.busy = self.assignments.is_some() && self.working > 0;
        if self.assignments.is_none() {
            self.working = 0;
        }
        if self.shown != (self.busy, self.working) {
            self.shown = (self.busy, self.working);
            self.changed = true;
        }

        self.expiry = self.assignments.as_ref().and_then(|assignments| {
            assignments
                .settled
                .iter()
                .map(|row| row.since + self.grace)
                .filter(|at| *at > now_ms)
                .min_by(f64::total_cmp)
        });
    }

    fn read_window(&mut self) {
        self.window_at = Some(Instant::now());
        let (window, monitors) = hypr::read_for_state(self.desktop.as_ref());
        let running = hypr::state(window.as_ref(), &monitors) != hypr::State::Absent;
        self.launching = launching_after(self.launching, running, Instant::now());
        let mut window = with_launch_flags(window_json(window.as_ref(), &monitors), self.launching);
        window["installed"] = json!(running || launch::installed());
        if window != self.window {
            self.window = window;
            self.changed = true;
        }
    }

    fn line(&self) -> Option<String> {
        let snapshot = json!({
            "v": 1,
            "ok": self.assignments.is_some(),
            "window": self.window,
            "alert": self.alert,
            "busy": self.busy,
            "working": self.working,
            "bots": self.bots,
            "costs": self.costs,
        });
        serde_json::to_string(&snapshot).ok()
    }
}

fn window_json(window: Option<&Window>, monitors: &[Monitor]) -> Value {
    json!({
        "state": hypr::state(window, monitors).name(),
        "address": window.map(|window| window.address.clone()).unwrap_or_default(),
    })
}

fn launching_after(started: Option<Instant>, running: bool, now: Instant) -> Option<Instant> {
    started.filter(|at| !running && now.saturating_duration_since(*at) < LAUNCH_TIMEOUT)
}

fn with_launch_flags(mut window: Value, launching: Option<Instant>) -> Value {
    let running = window["state"] != "absent";
    window["running"] = json!(running);
    window["launching"] = json!(launching.is_some());
    window
}

struct Silent;

impl Notifier for Silent {
    fn send(&self, _body: &[u8], _icon: Option<&OsStr>) {}
}

enum Inbox {
    Order(String),
    Shut,
}

fn reader() -> Receiver<Inbox> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut line = String::new();
        loop {
            line.clear();
            match stdin.lock().read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let _ = sender.send(Inbox::Shut);
                    return;
                }
                Ok(_) => {
                    if sender.send(Inbox::Order(line.trim().to_string())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    receiver
}

fn write_line(line: &str) -> io::Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

fn error_line(panic: &(dyn std::any::Any + Send)) -> String {
    let message = panic
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "engine error".to_string());
    json!({"v": 1, "error": message.chars().take(200).collect::<String>()}).to_string()
}

pub fn run(options: Options) -> i32 {
    let mut engine = Engine::new(&options);

    if options.once {
        return match engine.tick(true) {
            Some(line) => {
                if write_line(&line).is_ok() {
                    0
                } else {
                    1
                }
            }
            None => 1,
        };
    }

    std::panic::set_hook(Box::new(|_| {}));
    let parent = std::os::unix::process::parent_id();
    let inbox = reader();
    let mut wake = Instant::now();
    let mut settle: Option<Instant> = None;

    loop {
        if std::os::unix::process::parent_id() != parent {
            return 0;
        }
        let force = settle.is_some_and(|at| Instant::now() >= at);
        if force {
            settle = None;
        }
        if force || Instant::now() >= wake {
            let line = match catch_unwind(AssertUnwindSafe(|| engine.tick(force))) {
                Ok(line) => line,
                Err(panic) => Some(error_line(panic.as_ref())),
            };
            if let Some(line) = line {
                if write_line(&line).is_err() {
                    return 0;
                }
            }
            wake = Instant::now() + TICK;
        }

        let next = settle.map_or(wake, |at| at.min(wake));
        let wait = next.saturating_duration_since(Instant::now());
        match inbox.recv_timeout(wait) {
            Ok(Inbox::Order(text)) => {
                engine.order(&text);
                settle = Some(Instant::now() + SETTLE);
            }
            Ok(Inbox::Shut) | Err(RecvTimeoutError::Disconnected) => return 0,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::hypr::{clients, monitors};

    fn saved(case: &str) -> (String, String) {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hypr").join(case);
        (
            std::fs::read_to_string(directory.join("clients.json")).expect("clients fixture"),
            std::fs::read_to_string(directory.join("monitors.json")).expect("monitors fixture"),
        )
    }

    #[test]
    fn every_saved_window_case_reads_as_the_dock_reports_it() {
        for (case, state, address) in [
            ("visible", "visible", "0x1000003"),
            ("parked-hidden", "hidden", "0x1000003"),
            ("parked-shown", "visible", "0x1000003"),
            ("absent", "absent", ""),
            ("other-special", "hidden", "0x1000003"),
            ("two-monitors", "visible", "0x1000003"),
        ] {
            let (clients_json, monitors_json) = saved(case);
            let window = hypr::window(&clients(Some(&clients_json)));
            let monitors = monitors(Some(&monitors_json));
            let value = window_json(window.as_ref(), &monitors);
            assert_eq!(value["state"], state, "{case}");
            assert_eq!(value["address"], address, "{case}");
        }
    }

    #[test]
    fn the_launch_state_is_running_launching_or_neither() {
        let absent = json!({"state": "absent", "address": ""});
        let visible = json!({"state": "visible", "address": "0x1000003"});
        let hidden = json!({"state": "hidden", "address": "0x1000003"});
        let now = Instant::now();

        let idle = with_launch_flags(absent.clone(), None);
        assert_eq!((idle["running"].as_bool(), idle["launching"].as_bool()), (Some(false), Some(false)));
        let waiting = with_launch_flags(absent, Some(now));
        assert_eq!((waiting["running"].as_bool(), waiting["launching"].as_bool()), (Some(false), Some(true)));
        for window in [visible, hidden] {
            let up = with_launch_flags(window, None);
            assert_eq!(up["running"].as_bool(), Some(true), "parked still counts as loaded");
        }

        assert_eq!(launching_after(Some(now), false, now), Some(now));
        assert_eq!(launching_after(Some(now), true, now), None);
        assert_eq!(launching_after(Some(now), false, now + LAUNCH_TIMEOUT), None);
        assert_eq!(launching_after(None, false, now), None);
    }

    #[test]
    fn a_shifted_clock_reads_the_moment_it_was_given() {
        let reference: Timestamp = "2026-11-02T00:00:05-03:00".parse().expect("timestamp");
        let clock = Shifted::to(reference);
        let drift = clock.now().as_millisecond() - reference.as_millisecond();
        assert!((0..2000).contains(&drift), "the shifted clock starts at the given moment, {drift} ms off");
    }

    #[test]
    fn a_tick_that_panics_is_one_line_the_hub_ignores() {
        let panic: &(dyn std::any::Any + Send) = &"the database went away";
        let value: Value = serde_json::from_str(&error_line(panic)).expect("json");
        assert_eq!(value["v"], 1);
        assert_eq!(value["error"], "the database went away");
        let long: &(dyn std::any::Any + Send) = &"x".repeat(400);
        let value: Value = serde_json::from_str(&error_line(long)).expect("json");
        assert_eq!(value["error"].as_str().unwrap().chars().count(), 200);
    }

    #[test]
    fn a_stamp_notices_a_rewrite_even_when_the_size_is_the_same() {
        let dir = crate::paths::scratch::TempDir::new("stream-stamp");
        let path = dir.join("file");
        std::fs::write(&path, b"aaaa").expect("write");
        let first = stamp_of(&path).expect("stamp");
        std::fs::write(&path, b"bbbb").expect("rewrite");
        let second = stamp_of(&path).expect("stamp");
        assert_eq!(first.size, second.size);
        assert_ne!(first, second, "a same-size rewrite moves the mtime");
        std::fs::remove_file(&path).expect("remove");
        assert_eq!(stamp_of(&path), None, "a missing file has no stamp");
    }
}
