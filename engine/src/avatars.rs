use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use crate::paths;

const PRUNE_GRACE: Duration = Duration::from_secs(60);

static PRUNED: AtomicU64 = AtomicU64::new(0);

pub fn pruned() -> u64 {
    PRUNED.load(Ordering::Relaxed)
}

pub fn directory() -> Option<PathBuf> {
    paths::runtime_subdir("team-avatars")
}

pub fn name_for(svg: &str) -> String {
    let digest = sha1_smol::Sha1::from(svg.as_bytes()).digest().to_string();
    format!("{}.svg", &digest[..16])
}

fn is_avatar_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".svg") else {
        return false;
    };
    stem.len() == 16 && stem.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn publish(svg: &str) -> String {
    match directory() {
        Some(directory) => publish_into(&directory, svg),
        None => String::new(),
    }
}

fn publish_into(directory: &Path, svg: &str) -> String {
    if svg.is_empty() {
        return String::new();
    }
    let path = directory.join(name_for(svg));

    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => return String::new(),
        Ok(metadata) if metadata.is_file() && metadata.len() == svg.len() as u64 => return file_url(&path),
        _ => {}
    }

    if paths::write_atomically(directory, &name_for(svg), svg.as_bytes()).is_err() {
        return String::new();
    }
    file_url(&path)
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

pub fn prune(keep: &HashSet<PathBuf>) {
    if let Some(directory) = directory() {
        prune_in(&directory, keep);
    }
}

fn prune_in(directory: &Path, keep: &HashSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_avatar_name(name) {
            continue;
        }
        let path = directory.join(name);
        if keep.contains(&path) {
            continue;
        }
        let fresh = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age < PRUNE_GRACE);
        if fresh {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            PRUNED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use crate::paths::scratch::TempDir;

    fn names(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .expect("read")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).expect("metadata").mode() & 0o777
    }

    #[test]
    fn a_planted_symlink_is_refused_and_a_wider_mode_is_narrowed_to_0700() {
        use std::fs::DirBuilder;
        use std::os::unix::fs::DirBuilderExt;

        let temp = TempDir::new("guard");
        let owner = temp.owner();
        let directory = temp.0.join("mission").join("team-avatars");
        DirBuilder::new().recursive(true).mode(0o700).create(&directory).expect("directory");

        assert!(paths::accept_dir(&directory, owner).is_some());
        let url = publish_into(&directory, "<svg/>");
        assert!(url.starts_with("file://"), "expected a picture, got {url:?}");
        assert!(Path::new(url.trim_start_matches("file://")).exists());

        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).expect("mode");
        assert!(
            paths::accept_dir(&directory, owner).is_some(),
            "a 0755 avatar directory of ours must be narrowed, not refused"
        );
        assert_eq!(mode(&directory), 0o700, "the wider mode must be gone from the directory itself");
        assert!(publish_into(&directory, "<svg>after</svg>").starts_with("file://"));

        let elsewhere = temp.0.join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("elsewhere");
        fs::rename(&directory, elsewhere.join("moved")).expect("move");
        std::os::unix::fs::symlink(elsewhere.join("moved"), &directory).expect("symlink");
        assert!(paths::accept_dir(&directory, owner).is_none(), "a symlinked avatar directory must be refused");
        assert_eq!(mode(&elsewhere.join("moved")), 0o700, "a planted symlink must leave its target alone");
        fs::remove_file(&directory).expect("unlink");

        fs::set_permissions(elsewhere.join("moved"), fs::Permissions::from_mode(0o755)).expect("mode");
        assert!(paths::accept_dir(&elsewhere.join("moved"), owner.wrapping_add(1)).is_none());
        assert_eq!(mode(&elsewhere.join("moved")), 0o755, "another owner's directory must not be chmodded");

        assert!(paths::accept_dir(&temp.0.join("nothing-here"), owner).is_none());
    }

    fn age(path: &Path) {
        let file = fs::File::options().write(true).open(path).expect("picture");
        let old = SystemTime::now() - Duration::from_secs(120);
        file.set_times(fs::FileTimes::new().set_modified(old)).expect("mtime");
    }

    #[test]
    fn a_picture_is_named_by_its_content_and_old_ones_are_pruned() {
        let temp = TempDir::new("prune");
        let first = publish_into(&temp.0, "<svg>one</svg>");
        let second = publish_into(&temp.0, "<svg>two</svg>");
        assert_ne!(first, second);
        assert_eq!(names(&temp.0).len(), 2);

        assert_eq!(publish_into(&temp.0, "<svg>one</svg>"), first);
        assert_eq!(names(&temp.0).len(), 2);
        assert_eq!(first.trim_start_matches("file://"), temp.0.join(name_for("<svg>one</svg>")).to_string_lossy());

        for url in [&first, &second] {
            age(Path::new(url.trim_start_matches("file://")));
        }
        let keep = HashSet::from([PathBuf::from(first.trim_start_matches("file://"))]);
        prune_in(&temp.0, &keep);
        assert_eq!(names(&temp.0), vec![name_for("<svg>one</svg>")]);

        assert!(is_avatar_name(&name_for("<svg/>")));
        assert!(!is_avatar_name("robot.svg"));
        assert!(!is_avatar_name("0123456789abcdef.svg.tmp"));
        assert!(!is_avatar_name("0123456789abcde.svg"));
    }

    #[test]
    fn a_fresh_picture_is_never_pruned_away() {
        let temp = TempDir::new("fresh");
        publish_into(&temp.0, "<svg>fresh</svg>");
        prune_in(&temp.0, &HashSet::new());
        assert_eq!(names(&temp.0).len(), 1, "a picture just written must survive this read");
    }

    #[test]
    fn without_a_runtime_directory_there_are_no_pictures() {
        assert_eq!(publish_into(Path::new("/nonexistent/omb-avatars"), "<svg/>"), "");
    }
}
