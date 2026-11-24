use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::paths;

const STAMP_LIMIT: u64 = 4096;

pub fn pending() -> bool {
    paths::runtime_dir().as_deref().is_some_and(pending_in)
}

pub fn ack() -> bool {
    paths::runtime_dir().as_deref().is_some_and(|directory| ack_in(directory).is_ok())
}

pub fn stamp_in(directory: &Path) -> io::Result<()> {
    paths::write_atomically(directory, "alert", format!("{}\n", nanoseconds()).as_bytes())
}

fn nanoseconds() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}{:09}", now.as_secs(), now.subsec_nanos())
}

fn stamp(path: &Path) -> Option<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(paths::O_NOFOLLOW | paths::O_CLOEXEC)
        .open(path)
        .ok()?;
    if let Ok(metadata) = file.metadata() {
        if metadata.is_file() && metadata.permissions().mode() & 0o077 != 0 {
            let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        }
    }
    let mut text = String::new();
    file.take(STAMP_LIMIT).read_to_string(&mut text).ok()?;
    let line = text.split('\n').next().unwrap_or("").trim_end();
    (!line.is_empty()).then(|| line.to_string())
}

pub fn pending_in(directory: &Path) -> bool {
    let Some(announced) = stamp(&directory.join("alert")) else {
        return false;
    };
    stamp(&directory.join("seen")).as_deref() != Some(announced.as_str())
}

pub fn ack_in(directory: &Path) -> io::Result<()> {
    let announced = stamp(&directory.join("alert")).unwrap_or_default();
    paths::write_atomically(directory, "seen", format!("{announced}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    use crate::paths::scratch::TempDir;

    fn write(directory: &Path, name: &str, text: &str) {
        fs::write(directory.join(name), text).expect("write");
    }

    #[test]
    fn a_stamp_is_pending_until_it_is_copied_into_seen() {
        let temp = TempDir::new("ack");
        let line = "1761955200123456789";

        assert!(!pending_in(&temp.0));

        write(&temp.0, "alert", &format!("{line}\n"));
        assert!(pending_in(&temp.0));

        assert!(ack_in(&temp.0).is_ok());
        assert!(!pending_in(&temp.0));
        assert_eq!(fs::read_to_string(temp.0.join("seen")).expect("seen"), format!("{line}\n"));
        assert!(ack_in(&temp.0).is_ok());
        assert!(!pending_in(&temp.0));

        write(&temp.0, "alert", "1761955200987654321\n");
        assert!(pending_in(&temp.0));

        write(&temp.0, "alert", "");
        assert!(!pending_in(&temp.0));

        assert!(!temp.0.join(format!("seen.tmp.{}", std::process::id())).exists());
    }

    #[test]
    fn a_stamp_is_a_nanosecond_number_the_icon_can_show() {
        let temp = TempDir::new("stamp-now");
        assert!(stamp_in(&temp.0).is_ok());

        let text = fs::read_to_string(temp.0.join("alert")).expect("alert");
        assert!(text.ends_with('\n'), "one line, as printf printed it: {text:?}");
        let value: u64 = text.trim().parse().expect("nanoseconds since the epoch");
        assert!(value > 1_700_000_000_000_000_000, "after 2023: {value}");
        assert!(pending_in(&temp.0));

        assert!(stamp_in(&temp.0).is_ok());
        let newer = fs::read_to_string(temp.0.join("alert")).expect("alert");
        assert!(newer.trim().parse::<u64>().expect("number") >= value);

        let outside = temp.0.join("outside");
        write(&temp.0, "outside", "1761955200123456789\n");
        fs::remove_file(temp.0.join("alert")).expect("remove");
        std::os::unix::fs::symlink(&outside, temp.0.join("alert")).expect("symlink");
        assert!(stamp_in(&temp.0).is_ok());
        assert_eq!(fs::read_to_string(&outside).expect("outside"), "1761955200123456789\n");
        assert!(!fs::symlink_metadata(temp.0.join("alert")).expect("alert").file_type().is_symlink());
    }

    #[test]
    fn acking_without_an_alert_leaves_an_empty_seen() {
        let temp = TempDir::new("empty");
        write(&temp.0, "seen", "1761955200000000000\n");
        assert!(ack_in(&temp.0).is_ok());
        assert_eq!(fs::read_to_string(temp.0.join("seen")).expect("seen"), "\n");
        assert!(!pending_in(&temp.0));
    }

    #[test]
    fn a_symlinked_stamp_is_never_followed() {
        let temp = TempDir::new("symlink");
        let outside = temp.0.join("outside");
        write(&temp.0, "outside", "1761955200123456789\n");
        std::os::unix::fs::symlink(&outside, temp.0.join("alert")).expect("symlink");

        assert!(!pending_in(&temp.0));
        assert!(ack_in(&temp.0).is_ok());
        assert_eq!(
            fs::read_to_string(&outside).expect("outside"),
            "1761955200123456789\n",
            "the file a symlink points at must not be read or rewritten"
        );
        assert_eq!(fs::read_to_string(temp.0.join("seen")).expect("seen"), "\n");
    }

    #[test]
    fn a_stamp_left_readable_by_others_is_narrowed() {
        let temp = TempDir::new("narrow");
        write(&temp.0, "alert", "1761955200123456789\n");
        fs::set_permissions(temp.0.join("alert"), fs::Permissions::from_mode(0o644)).expect("mode");
        assert!(pending_in(&temp.0));
        let mode = fs::metadata(temp.0.join("alert")).expect("alert").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
