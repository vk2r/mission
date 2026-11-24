use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::time::{Duration, Instant};

use jiff::{Timestamp, civil};
use serde::Deserialize;
use serde_json::Value;

pub const FILE_LIMIT: u64 = 64 << 20;

const GUARD: u64 = 4096;

const RECHECK: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone)]
pub enum At {
    Instant(Timestamp),
    Local(String),
}

#[derive(Debug, Clone)]
pub struct Rec {
    pub at: At,
    pub cost: Option<f64>,
    pub bot_id: Option<String>,
    pub bot_name: Option<String>,
    pub model: Option<String>,
    pub ok: bool,
}

#[derive(Deserialize)]
struct Line {
    at: Option<Value>,
    #[serde(rename = "costUsd")]
    cost: Option<Value>,
    #[serde(rename = "botId")]
    bot_id: Option<Value>,
    #[serde(rename = "botName")]
    bot_name: Option<Value>,
    model: Option<Value>,
    outcome: Option<Value>,
}

pub fn parse_line(line: &str) -> Option<Rec> {
    let parsed: Line = serde_json::from_str(line).ok()?;
    let at = classify(parsed.at.as_ref()?.as_str()?)?;
    let text = |value: &Option<Value>| value.as_ref().and_then(Value::as_str).map(str::to_owned);
    let bot_id = text(&parsed.bot_id);
    let bot_name = crate::usage::bot_name(parsed.bot_name.as_ref().and_then(Value::as_str), bot_id.as_deref());
    Some(Rec {
        at,
        cost: parsed.cost.as_ref().and_then(Value::as_f64),
        bot_name,
        bot_id,
        model: parsed.model.as_ref().and_then(Value::as_str).and_then(crate::usage::model_name),
        ok: parsed.outcome.as_ref().and_then(|outcome| outcome.get("status")).and_then(Value::as_str) == Some("ok"),
    })
}

fn classify(stamp: &str) -> Option<At> {
    let stamp = stamp.replace('Z', "+00:00");
    if let Ok(timestamp) = stamp.parse::<Timestamp>() {
        return Some(At::Instant(timestamp));
    }
    if stamp.parse::<civil::DateTime>().is_ok() || stamp.parse::<civil::Date>().is_ok() {
        return Some(At::Local(stamp));
    }
    if stamp.as_bytes().get(10) == Some(&b' ') {
        let mut fixed = stamp;
        fixed.replace_range(10..11, "T");
        if let Ok(timestamp) = fixed.parse::<Timestamp>() {
            return Some(At::Instant(timestamp));
        }
    }
    None
}

struct Tracked {
    device: u64,
    inode: u64,
    mtime: (i64, i64),
    size: u64,
    offset: u64,
    guard: u64,
    records: Vec<Rec>,
    pending: Option<Rec>,
    full_at: Instant,
}

pub struct Ledger {
    files: HashMap<String, Tracked>,
    refused: HashSet<String>,
    capped: HashSet<String>,
    pub bytes_read: u64,
    limit: u64,
}

impl Default for Ledger {
    fn default() -> Self {
        Ledger { files: HashMap::new(), refused: HashSet::new(), capped: HashSet::new(), bytes_read: 0, limit: FILE_LIMIT }
    }
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn with_limit(limit: u64) -> Self {
        Ledger { limit, ..Self::default() }
    }

    pub fn refresh(&mut self, directory: &Path, names: &[String]) -> std::io::Result<()> {
        self.files.retain(|name, _| names.contains(name));
        self.capped.retain(|name| names.contains(name));
        for name in names {
            self.refresh_file(directory, name)?;
        }
        Ok(())
    }

    pub fn records<'a>(&'a self, names: &'a [String]) -> impl Iterator<Item = &'a Rec> {
        names
            .iter()
            .filter_map(|name| self.files.get(name))
            .flat_map(|file| file.records.iter().chain(file.pending.iter()))
    }

    fn refuse(&mut self, name: &str, why: &str) {
        self.files.remove(name);
        if self.refused.insert(name.to_string()) {
            eprintln!("mausbot: usage/{name:?} ignored: {why}");
        }
    }

    fn refresh_file(&mut self, directory: &Path, name: &str) -> std::io::Result<()> {
        let path = directory.join(name);
        let file = match OpenOptions::new().read(true).custom_flags(crate::paths::READ_FLAGS).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.files.remove(name);
                return Ok(());
            }
            Err(error) if error.raw_os_error() == Some(40) => {
                self.refuse(name, "it is a symlink");
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            self.refuse(name, "it is not a regular file");
            return Ok(());
        }
        self.refused.remove(name);
        let length = metadata.len();
        let mtime = (metadata.mtime(), metadata.mtime_nsec());

        let mut from = 0u64;
        let mut incremental = false;
        if let Some(tracked) = self.files.get(name) {
            let same = tracked.device == metadata.dev() && tracked.inode == metadata.ino();
            if same && length == tracked.size && mtime == tracked.mtime {
                return Ok(());
            }
            let grown = same && length > tracked.size && tracked.full_at.elapsed() < RECHECK;
            if grown && guard_of(&file, tracked.offset) == Some(tracked.guard) {
                self.bytes_read += tracked.offset.min(GUARD);
                from = tracked.offset;
                incremental = true;
            }
        }

        let limit = self.limit.saturating_sub(from);
        if length <= self.limit {
            self.capped.remove(name);
        } else if self.capped.insert(name.to_string()) {
            eprintln!("mausbot: usage/{name:?} is over {} bytes, only the first part is read", self.limit);
        }
        let mut buffer = Vec::new();
        let mut handle = &file;
        handle.seek(SeekFrom::Start(from))?;
        handle.take(limit).read_to_end(&mut buffer)?;
        self.bytes_read += buffer.len() as u64;

        let end = buffer.iter().rposition(|byte| *byte == b'\n').map_or(0, |at| at + 1);
        let mut fresh: Vec<Rec> = Vec::new();
        for line in buffer[..end].split(|byte| *byte == b'\n') {
            if let Some(record) = parse_line(&String::from_utf8_lossy(line)) {
                fresh.push(record);
            }
        }
        let pending = parse_line(&String::from_utf8_lossy(&buffer[end..]));
        let offset = from + end as u64;

        let mut tracked = match incremental.then(|| self.files.remove(name)).flatten() {
            Some(tracked) => tracked,
            None => Tracked {
                device: metadata.dev(),
                inode: metadata.ino(),
                mtime,
                size: 0,
                offset: 0,
                guard: 0,
                records: Vec::new(),
                pending: None,
                full_at: Instant::now(),
            },
        };
        tracked.records.extend(fresh);
        tracked.pending = pending;
        tracked.mtime = mtime;
        tracked.size = length.min(from + buffer.len() as u64);
        tracked.offset = offset;
        tracked.guard = guard_of(&file, offset).unwrap_or(0);
        self.files.insert(name.to_string(), tracked);
        Ok(())
    }
}

fn guard_of(file: &std::fs::File, offset: u64) -> Option<u64> {
    let start = offset.saturating_sub(GUARD);
    let mut bytes = vec![0u8; (offset - start) as usize];
    file.read_exact_at(&mut bytes, start).ok()?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some(hasher.finish())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::*;
    use crate::paths::scratch::TempDir;

    fn line(cost: &str, bot: &str) -> String {
        format!("{{\"at\":\"2026-11-01T10:00:00Z\",\"costUsd\":{cost},\"botId\":\"{bot}\",\"botName\":\"{bot}\",\"model\":\"m\"}}\n")
    }

    fn scratch(name: &str) -> TempDir {
        TempDir::new(&format!("ledger-{name}"))
    }

    fn costs(ledger: &Ledger, names: &[String]) -> Vec<Option<f64>> {
        ledger.records(names).map(|record| record.cost).collect()
    }

    fn fresh(dir: &Path, names: &[String]) -> Vec<Option<f64>> {
        let mut ledger = Ledger::new();
        ledger.refresh(dir, names).expect("refresh");
        costs(&ledger, names)
    }

    #[test]
    fn an_append_reads_only_the_new_lines_and_equals_a_full_reread() {
        let dir = scratch("append");
        let names = vec!["a.jsonl".to_string()];
        fs::write(dir.join("a.jsonl"), line("0.1", "b1") + &line("0.2", "b1")).expect("write");
        let mut ledger = Ledger::new();
        ledger.refresh(&dir, &names).expect("first");
        let first = ledger.bytes_read;
        let mut file = OpenOptions::new().append(true).open(dir.join("a.jsonl")).expect("open");
        file.write_all(line("0.3", "b2").as_bytes()).expect("append");
        ledger.refresh(&dir, &names).expect("second");
        assert_eq!(costs(&ledger, &names), fresh(&dir, &names));
        assert_eq!(costs(&ledger, &names), [Some(0.1), Some(0.2), Some(0.3)]);
        assert!(ledger.bytes_read - first < 2 * line("0.3", "b2").len() as u64 + GUARD, "only the tail is read");
    }

    #[test]
    fn a_line_finished_later_counts_once() {
        let dir = scratch("pending");
        let names = vec!["a.jsonl".to_string()];
        let whole = line("0.5", "b1");
        let (head, tail) = whole.split_at(whole.len() - 5);
        fs::write(dir.join("a.jsonl"), head).expect("write");
        let mut ledger = Ledger::new();
        ledger.refresh(&dir, &names).expect("first");
        assert!(costs(&ledger, &names).is_empty());
        let mut file = OpenOptions::new().append(true).open(dir.join("a.jsonl")).expect("open");
        file.write_all(tail.as_bytes()).expect("finish");
        ledger.refresh(&dir, &names).expect("second");
        assert_eq!(costs(&ledger, &names), [Some(0.5)]);
        assert_eq!(costs(&ledger, &names), fresh(&dir, &names));
    }

    #[test]
    fn a_truncated_or_replaced_file_is_read_from_the_start() {
        let dir = scratch("truncate");
        let names = vec!["a.jsonl".to_string()];
        let path = dir.join("a.jsonl");
        fs::write(&path, line("0.1", "b1") + &line("0.2", "b1") + &line("0.3", "b1")).expect("write");
        let mut ledger = Ledger::new();
        ledger.refresh(&dir, &names).expect("first");
        fs::write(&path, line("0.9", "b1")).expect("truncate");
        ledger.refresh(&dir, &names).expect("second");
        assert_eq!(costs(&ledger, &names), [Some(0.9)]);
        fs::remove_file(&path).expect("remove");
        fs::write(&path, line("0.4", "b2") + &line("0.5", "b2") + &line("0.6", "b2") + &line("0.7", "b2")).expect("replace");
        ledger.refresh(&dir, &names).expect("third");
        assert_eq!(costs(&ledger, &names), fresh(&dir, &names));
        assert_eq!(costs(&ledger, &names).len(), 4);
    }

    #[test]
    fn a_line_edited_in_place_is_noticed() {
        let dir = scratch("edit");
        let names = vec!["a.jsonl".to_string()];
        let path = dir.join("a.jsonl");
        fs::write(&path, line("null", "b1") + &line("0.2", "b1")).expect("write");
        let mut ledger = Ledger::new();
        ledger.refresh(&dir, &names).expect("first");
        fs::write(&path, line("0.25", "b1") + &line("0.2", "b1") + &line("0.1", "b1")).expect("edit");
        ledger.refresh(&dir, &names).expect("second");
        assert_eq!(costs(&ledger, &names), fresh(&dir, &names));
        assert_eq!(costs(&ledger, &names), [Some(0.25), Some(0.2), Some(0.1)]);
    }

    fn age(ledger: &mut Ledger, name: &str, by: Duration) {
        let tracked = ledger.files.get_mut(name).expect("tracked");
        tracked.full_at = Instant::now().checked_sub(by).expect("instant");
    }

    #[test]
    fn an_append_long_after_the_last_full_read_is_still_incremental() {
        let dir = scratch("late");
        let names = vec!["a.jsonl".to_string()];
        let path = dir.join("a.jsonl");
        fs::write(&path, line("0.1", "b1").repeat(2000)).expect("write");
        let mut ledger = Ledger::new();
        ledger.refresh(&dir, &names).expect("first");
        age(&mut ledger, "a.jsonl", Duration::from_secs(61));
        let before = ledger.bytes_read;
        let mut file = OpenOptions::new().append(true).open(&path).expect("open");
        file.write_all(line("0.3", "b2").as_bytes()).expect("append");
        ledger.refresh(&dir, &names).expect("second");
        assert!(ledger.bytes_read - before < 2 * line("0.3", "b2").len() as u64 + GUARD, "only the tail is read");
        assert_eq!(costs(&ledger, &names), fresh(&dir, &names));
        assert_eq!(costs(&ledger, &names).len(), 2001);
    }

    #[test]
    fn an_edit_far_back_is_caught_by_the_net() {
        let dir = scratch("far");
        let names = vec!["a.jsonl".to_string()];
        let path = dir.join("a.jsonl");
        fs::write(&path, line("0.1", "b1").repeat(2000)).expect("write");
        let mut ledger = Ledger::new();
        ledger.refresh(&dir, &names).expect("first");
        let mut bytes = fs::read(&path).expect("read");
        bytes[..line("0.1", "b1").len()].copy_from_slice(line("0.9", "b1").as_bytes());
        fs::write(&path, &bytes).expect("edit");
        let mut file = OpenOptions::new().append(true).open(&path).expect("open");
        file.write_all(line("0.3", "b2").as_bytes()).expect("append");
        age(&mut ledger, "a.jsonl", RECHECK + Duration::from_secs(1));
        ledger.refresh(&dir, &names).expect("second");
        assert_eq!(costs(&ledger, &names), fresh(&dir, &names));
        assert_eq!(costs(&ledger, &names)[0], Some(0.9));
    }

    #[test]
    fn a_month_change_forgets_the_files_that_are_no_longer_watched() {
        let dir = scratch("month");
        fs::write(dir.join("2026-10.jsonl"), line("1.0", "b1")).expect("oct");
        fs::write(dir.join("2026-11.jsonl"), line("2.0", "b1")).expect("nov");
        let mut ledger = Ledger::new();
        let october = vec!["2026-10.jsonl".to_string()];
        ledger.refresh(&dir, &october).expect("october");
        assert_eq!(costs(&ledger, &october), [Some(1.0)]);
        let november = vec!["2026-11.jsonl".to_string()];
        ledger.refresh(&dir, &november).expect("november");
        assert_eq!(costs(&ledger, &november), [Some(2.0)]);
        assert!(costs(&ledger, &october).is_empty());
        assert_eq!(costs(&ledger, &november), fresh(&dir, &november));
    }

    #[test]
    fn a_symlinked_month_file_is_never_followed() {
        let dir = scratch("symlink");
        let secret = dir.join("elsewhere.txt");
        fs::write(&secret, line("99", "evil")).expect("target");
        std::os::unix::fs::symlink(&secret, dir.join("a.jsonl")).expect("link");
        let names = vec!["a.jsonl".to_string()];
        let mut ledger = Ledger::new();
        ledger.refresh(&dir, &names).expect("a symlink is skipped, not an error");
        assert!(costs(&ledger, &names).is_empty());
        assert_eq!(ledger.bytes_read, 0);
    }

    #[test]
    fn a_fifo_in_place_of_a_month_file_does_not_block() {
        let dir = scratch("fifo");
        assert!(std::process::Command::new("mkfifo").arg(dir.join("a.jsonl")).status().expect("mkfifo").success());
        let names = vec!["a.jsonl".to_string()];
        let mut ledger = Ledger::new();
        ledger.refresh(&dir, &names).expect("a fifo is skipped, not an error");
        assert!(costs(&ledger, &names).is_empty());
        assert_eq!(ledger.bytes_read, 0);
    }

    #[test]
    fn a_file_over_the_cap_is_read_only_up_to_it() {
        let dir = scratch("cap");
        let one = line("1.0", "b1");
        let names = vec!["a.jsonl".to_string()];
        fs::write(dir.join("a.jsonl"), one.repeat(10)).expect("write");
        let mut ledger = Ledger::with_limit(one.len() as u64 * 3 + 7);
        ledger.refresh(&dir, &names).expect("refresh");
        assert_eq!(costs(&ledger, &names).len(), 3, "the lines inside the cap count, the cut line does not");
        assert!(ledger.bytes_read <= one.len() as u64 * 3 + 7);
    }
}
