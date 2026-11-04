use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions, Permissions, TryLockError};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::usage::clean;
use crate::{alert, launch};

const TURN_MARK: &[u8] = b"[omb-turn]";
const DEPTH_MARK: &[u8] = b"depth=";
const TEXT_OPEN: &[u8] = b"text=\"";
const LIMIT: usize = 120;
const CUT: usize = 116;
const CHUNK: u64 = 64 * 1024;

#[derive(Debug, Default, Clone)]
pub struct Tail {
    offset: u64,
    partial: Vec<u8>,
}

impl Tail {
    pub fn at_end(size: u64) -> Self {
        Self { offset: size, partial: Vec::new() }
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn restart(&mut self) {
        self.offset = 0;
        self.partial.clear();
    }

    pub fn rotated(&mut self, size: u64) {
        self.offset = size;
        self.partial.clear();
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.offset += chunk.len() as u64;
        self.partial.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let rest = self.partial.split_off(end + 1);
            let mut line = std::mem::replace(&mut self.partial, rest);
            line.pop();
            lines.push(line);
        }
        lines
    }
}

pub fn size_of(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().map(|metadata| metadata.len())
}

pub fn advance(path: &Path, tail: &mut Tail) -> Vec<Vec<u8>> {
    let Some(size) = size_of(path) else {
        tail.restart();
        return Vec::new();
    };
    if size < tail.offset() {
        tail.rotated(size);
        return Vec::new();
    }
    if size == tail.offset() {
        return Vec::new();
    }
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    if file.seek(SeekFrom::Start(tail.offset())).is_err() {
        return Vec::new();
    }
    let mut chunk = Vec::new();
    let want = (size - tail.offset()).min(CHUNK);
    if file.take(want).read_to_end(&mut chunk).is_err() {
        return Vec::new();
    }
    tail.feed(&chunk)
}

pub fn is_event(line: &[u8]) -> bool {
    depth(line).is_some_and(|depth| depth >= 1)
}

pub fn depth(line: &[u8]) -> Option<u64> {
    if !contains(line, TURN_MARK) {
        return None;
    }
    let at = find_last(line, DEPTH_MARK)? + DEPTH_MARK.len();
    let digits = &line[at..];
    let end = digits.iter().position(|byte| !byte.is_ascii_digit()).unwrap_or(digits.len());
    let text = std::str::from_utf8(&digits[..end]).ok()?;
    text.parse::<u64>().ok()
}

pub fn notification_text(line: &[u8]) -> Vec<u8> {
    let field = match captured(line) {
        Some(text) if !text.is_empty() => text.to_vec(),
        _ => line.to_vec(),
    };
    let words = String::from_utf8_lossy(&unescape(&field)).split_whitespace().collect::<Vec<_>>().join(" ");
    let whole = clean(&words, usize::MAX).unwrap_or_default();
    if whole.len() <= LIMIT {
        return whole.into_bytes();
    }
    format!("{}...", clean(&whole, CUT).unwrap_or_default()).into_bytes()
}

fn captured(line: &[u8]) -> Option<&[u8]> {
    let mut starts = Vec::new();
    let mut from = 0;
    while let Some(found) = find(&line[from..], TEXT_OPEN) {
        let at = from + found;
        starts.push(at + TEXT_OPEN.len());
        from = at + 1;
    }
    starts.into_iter().rev().find_map(|body| {
        let rest = &line[body..];
        let end = closing_quote(rest)?;
        Some(&rest[..end])
    })
}

fn closing_quote(rest: &[u8]) -> Option<usize> {
    let mut index = 0;
    while index < rest.len() {
        match rest[index] {
            b'\\' if index + 1 < rest.len() => index += 2,
            b'\\' => return None,
            b'"' => return Some(index),
            _ => index += 1,
        }
    }
    None
}

fn unescape(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if text[index] == b'\\' && text.get(index + 1) == Some(&b'"') {
            out.push(b'"');
            index += 2;
        } else {
            out.push(text[index]);
            index += 1;
        }
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|window| window == needle)
}

pub trait Notifier {
    fn send(&self, body: &[u8], icon: Option<&OsStr>);
}

pub struct Desktop;

impl Notifier for Box<dyn Notifier> {
    fn send(&self, body: &[u8], icon: Option<&OsStr>) {
        (**self).send(body, icon);
    }
}

impl Notifier for Desktop {
    fn send(&self, body: &[u8], icon: Option<&OsStr>) {
        if which("notify-send").is_none() {
            return;
        }
        let mut command = Command::new("notify-send");
        command.args(notify_args(body, icon)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let _ = command.status();
    }
}

fn notify_args(body: &[u8], icon: Option<&OsStr>) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["-a".into(), "MausBot".into()];
    if let Some(icon) = icon {
        args.push("-i".into());
        args.push(icon.to_os_string());
    }
    args.push("--".into());
    args.push("MausBot".into());
    args.push(OsString::from_vec(escape_markup(body)));
    args
}

fn escape_markup(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for byte in text {
        match byte {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'\'' => out.extend_from_slice(b"&apos;"),
            b'"' => out.extend_from_slice(b"&quot;"),
            other => out.push(*other),
        }
    }
    out
}

pub struct Emitter<N> {
    directory: PathBuf,
    notifier: N,
}

impl<N: Notifier> Emitter<N> {
    pub fn new(directory: PathBuf, notifier: N) -> Self {
        Self { directory, notifier }
    }

    pub fn announce(&self, line: &[u8]) -> bool {
        if !is_event(line) {
            return false;
        }
        let _ = alert::stamp_in(&self.directory);
        let icon = launch::icon();
        self.notifier.send(&notification_text(line), icon.as_deref().map(OsStr::new));
        true
    }
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|directory| directory.join(program)).find(|candidate| {
        fs::metadata(candidate)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

pub fn claim(directory: &Path) -> Result<File, i32> {
    let path = directory.join("watch.lock");
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(crate::paths::O_NOFOLLOW | crate::paths::O_CLOEXEC)
        .open(&path)
        .map_err(|_| 1)?;
    if file.metadata().is_ok_and(|metadata| metadata.mode() & 0o077 != 0) {
        file.set_permissions(Permissions::from_mode(0o600)).map_err(|_| 1)?;
    }
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(3),
        Err(TryLockError::Error(_)) => Err(1),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_old_lock_with_open_permissions_is_narrowed() {
        let dir = crate::paths::scratch::TempDir::new("claim");
        fs::write(dir.join("watch.lock"), "").expect("old lock");
        fs::set_permissions(dir.join("watch.lock"), Permissions::from_mode(0o644)).expect("mode");
        let held = claim(&dir).expect("claimed");
        assert_eq!(held.metadata().expect("meta").mode() & 0o777, 0o600);
    }

    #[test]
    fn the_popup_body_is_escaped_and_cannot_pass_for_an_option() {
        let args = notify_args(b"<b>bold</b> & <a href=\"x\">it's</a>", None);
        let texts: Vec<String> = args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
        assert_eq!(texts[..4], ["-a", "MausBot", "--", "MausBot"]);
        assert_eq!(texts[4], "&lt;b&gt;bold&lt;/b&gt; &amp; &lt;a href=&quot;x&quot;&gt;it&apos;s&lt;/a&gt;");

        let args = notify_args(b"-t 0 -u critical", Some(OsStr::new("/tmp/icon.svg")));
        let texts: Vec<String> = args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
        assert_eq!(texts, ["-a", "MausBot", "-i", "/tmp/icon.svg", "--", "MausBot", "-t 0 -u critical"]);
    }

    use super::*;
    use std::cell::RefCell;
    use std::time::Duration;
    use std::path::PathBuf;

    use crate::paths::scratch::TempDir;

    fn turn(text: &str, depth: u32) -> String {
        format!(
            "[2026-11-01T03:36:55.419Z] [out] [omb-turn] bot=6d32f997-36f5-4802-adc6-349f42c76b3d text=\"{text}\" images=0 depth={depth} card=false\n"
        )
    }

    #[test]
    fn a_line_split_across_two_writes_is_one_event() {
        let first = b"[out] [omb-turn] text=\"half\"";
        let rest = b" depth=1\n";
        let whole: Vec<u8> = first
            .iter()
            .chain(rest.iter().filter(|byte| **byte != b'\n'))
            .copied()
            .collect();

        let mut tail = Tail::at_end(0);
        assert!(tail.feed(first).is_empty(), "no newline yet");
        assert_eq!(tail.offset(), first.len() as u64);

        assert_eq!(tail.feed(rest), vec![whole.clone()], "the second write completes the line");
        assert_eq!(tail.offset(), (first.len() + rest.len()) as u64);

        assert!(tail.feed(b"").is_empty());

        let lines = tail.feed(b"one\ntwo\nthree");
        assert_eq!(lines, vec![b"one".to_vec(), b"two".to_vec()]);
        assert_eq!(tail.offset(), (whole.len() + 1 + 13) as u64);
        assert_eq!(tail.partial, b"three".to_vec());
    }

    #[test]
    fn a_rotated_or_truncated_log_is_not_replayed() {
        let mut tail = Tail::at_end(5);
        assert_eq!(tail.feed(b"kept\nsecond\n"), vec![b"kept".to_vec(), b"second".to_vec()]);
        assert_eq!(tail.offset(), 17);

        tail.rotated(4);
        assert_eq!(tail.offset(), 4);
        assert_eq!(tail.feed(b"old\n"), vec![b"old".to_vec()]);
        assert_eq!(tail.feed(b"new\n"), vec![b"new".to_vec()]);

        let mut tail = Tail::at_end(0);
        assert!(tail.feed(b"a line that never ended").is_empty());
        tail.rotated(0);
        assert_eq!(tail.partial, Vec::<u8>::new());
        assert_eq!(tail.feed(b"fresh\n"), vec![b"fresh".to_vec()]);

        let mut tail = Tail::at_end(99);
        tail.restart();
        assert_eq!(tail.offset(), 0);
        assert_eq!(tail.feed(b"reborn\n"), vec![b"reborn".to_vec()]);
    }

    #[test]
    fn only_turns_started_by_someone_else_are_events() {
        assert!(is_event(turn("hola", 1).as_bytes()));
        assert!(is_event(turn("hola", 12).as_bytes()));
        assert!(is_event(b"[omb-turn] depth=007"));
        assert!(!is_event(turn("hola", 0).as_bytes()), "depth=0 is the user talking");
        assert!(!is_event(b"[omb-turn] no counter here"));
        assert!(!is_event(b"[omb-turn] depth="));
        assert!(!is_event(b"[omb-turn] depth=abc"));
        assert!(!is_event(b"[omb-turn] depth=18446744073709551616"), "not a number we can hold");
        assert!(!is_event(b"just a log line, depth=3"));

        assert!(!is_event(b"[omb-turn] depth=3 card=true depth=0"));
        assert!(is_event(b"[omb-turn] depth=0 card=false depth=2"));
        assert_eq!(depth(b"[omb-turn] depth=3 card=true"), Some(3));
    }

    #[test]
    fn the_text_is_the_field_with_escapes_and_blanks_folded() {
        assert_eq!(notification_text(b"[omb-turn] text=\"Bot finished\" depth=1"), b"Bot finished");
        assert_eq!(
            notification_text(b"[omb-turn] text=\"He said \\\"hola\\\" to me\" depth=1"),
            b"He said \"hola\" to me"
        );
        assert_eq!(
            notification_text(b"[omb-turn] text=\"   a\t\t b \r\n c  \" depth=1"),
            b"a b c"
        );
        assert_eq!(
            notification_text(b"[omb-turn] bot=x depth=1"),
            b"[omb-turn] bot=x depth=1"
        );
        assert_eq!(
            notification_text(b"[omb-turn] text=\"\" depth=1"),
            b"[omb-turn] text=\"\" depth=1"
        );
        assert_eq!(
            notification_text(b"[omb-turn] text=\"unclosed depth=1"),
            b"[omb-turn] text=\"unclosed depth=1"
        );
        assert_eq!(
            notification_text(b"[omb-turn] text=\"first\" then text=\"second\" depth=1"),
            b"second"
        );
        assert_eq!(
            notification_text(b"[omb-turn] text=\"a\\\"b\" then text=\"c\\\"d\" depth=1"),
            b"c\"d"
        );
        assert_eq!(
            notification_text(b"[omb-turn] text=\"no quote\\"),
            b"[omb-turn] text=\"no quote\\"
        );
    }

    #[test]
    fn the_cut_is_at_120_bytes_and_never_splits_a_character() {
        let exact = "a".repeat(120);
        assert_eq!(notification_text(turn(&exact, 1).as_bytes()), exact.as_bytes());

        let long = "b".repeat(121);
        let text = notification_text(turn(&long, 1).as_bytes());
        assert_eq!(text, format!("{}...", "b".repeat(116)).as_bytes());
        assert_eq!(text.len(), 119);

        let accented = format!("{}é{}", "a".repeat(115), "z".repeat(10));
        let text = notification_text(turn(&accented, 1).as_bytes());
        assert_eq!(text, format!("{}...", "a".repeat(115)).as_bytes());
        assert!(std::str::from_utf8(&text).is_ok(), "still valid UTF-8");

        let emoji = format!("{}😀{}", "a".repeat(114), "z".repeat(10));
        let text = notification_text(turn(&emoji, 1).as_bytes());
        assert_eq!(text, format!("{}...", "a".repeat(114)).as_bytes());
        assert!(std::str::from_utf8(&text).is_ok());

        let emoji = format!("{}😀{}", "a".repeat(112), "z".repeat(10));
        let text = notification_text(turn(&emoji, 1).as_bytes());
        assert_eq!(text, format!("{}😀...", "a".repeat(112)).as_bytes());
        assert!(std::str::from_utf8(&text).is_ok());
    }

    #[test]
    fn no_control_character_survives_the_popup() {
        let line = b"[omb-turn] text=\"a\x01b \x7f c\x1bd\" depth=1";
        let text = notification_text(line);
        assert_eq!(text, b"ab  cd");
        assert!(text.iter().all(|byte| *byte >= 0x20 && *byte != 0x7f));
        assert!(text.len() <= LIMIT);
    }

    struct Recorder {
        directory: PathBuf,
        sent: RefCell<Vec<(Vec<u8>, Option<String>, bool)>>,
    }

    impl Notifier for &Recorder {
        fn send(&self, body: &[u8], icon: Option<&OsStr>) {
            let stamped = fs::read_to_string(self.directory.join("alert"))
                .map(|stamp| !stamp.trim().is_empty() && stamp.trim().parse::<u64>().is_ok())
                .unwrap_or(false);
            self.sent.borrow_mut().push((body.to_vec(), icon.map(|i| i.to_string_lossy().into_owned()), stamped));
        }
    }

    #[test]
    fn the_alert_is_stamped_before_the_popup_goes_out() {
        let temp = TempDir::new("stamp");
        let recorder = Recorder { directory: temp.0.clone(), sent: RefCell::new(Vec::new()) };
        let emitter = Emitter::new(temp.0.clone(), &recorder);

        assert!(!emitter.announce(b"[omb-turn] text=\"depth 0\" depth=0"), "not an event");
        assert!(recorder.sent.borrow().is_empty());

        assert!(emitter.announce(turn("a bot answered", 2).as_bytes()));
        {
            let sent = recorder.sent.borrow();
            assert_eq!(sent.len(), 1);
            assert_eq!(sent[0].0, b"a bot answered");
            assert!(sent[0].2, "the alert has to be stamped when the popup is sent");
        }

        let stamp = fs::read_to_string(temp.0.join("alert")).expect("alert");
        assert!(stamp.trim().len() >= 19, "nanoseconds: {stamp}");
        assert!(alert::pending_in(&temp.0), "the icon shows it until it is seen");

        let first = stamp.trim().to_string();
        std::thread::sleep(Duration::from_millis(2));
        assert!(emitter.announce(turn("and again", 1).as_bytes()));
        let second = fs::read_to_string(temp.0.join("alert")).expect("alert");
        assert_ne!(first, second.trim());
        assert!(second.trim().parse::<u64>().expect("a number") > first.parse::<u64>().expect("a number"));
        assert!(!temp.0.join(format!("alert.tmp.{}", std::process::id())).exists(), "no leftover");
    }

    #[test]
    fn a_second_follower_is_told_to_come_back_later() {
        let temp = TempDir::new("lock");
        let first = claim(&temp.0).expect("the first follower takes the lock");
        assert_eq!(claim(&temp.0).unwrap_err(), 3, "another copy is following the log");
        drop(first);
        assert!(claim(&temp.0).is_ok(), "the lock is free again once that copy is gone");
        assert!(temp.0.join("watch.lock").exists());
    }
}
