use std::fs::{self, DirBuilder, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const O_DIRECTORY: i32 = 0o200000;
pub const O_NOFOLLOW: i32 = 0o400000;
pub const O_CLOEXEC: i32 = 0o2000000;
const O_NONBLOCK: i32 = 0o4000;

pub const READ_FLAGS: i32 = O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK;

pub fn home() -> Option<PathBuf> {
    match std::env::var_os("OPENMAUSBOT_HOME") {
        Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => user_home().map(|home| home.join(".openmausbot")),
    }
}

pub fn user_home() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|value| !value.is_empty()).map(PathBuf::from)
}

pub fn bots_file() -> Option<PathBuf> {
    home().map(|home| home.join("bots.json"))
}

pub fn db_file() -> Option<PathBuf> {
    home().map(|home| home.join("runtime.db"))
}

pub fn usage_dir() -> Option<PathBuf> {
    home().map(|home| home.join("usage"))
}

pub fn config_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => user_home().map(|home| home.join(".config")),
    }
}

pub fn log_file() -> Option<PathBuf> {
    match std::env::var_os("OPENMAUSBOT_LOG") {
        Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => config_home().map(|config| config.join("openmausbot").join("logs").join("server.log")),
    }
}

pub fn data_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => user_home().map(|home| home.join(".local").join("share")),
    }
}

fn runtime_base() -> Option<PathBuf> {
    let value = std::env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty())?;
    let base = PathBuf::from(value);
    let metadata = fs::symlink_metadata(&base).ok()?;
    (metadata.is_dir() && !metadata.file_type().is_symlink() && metadata.uid() == process_uid()?).then_some(base)
}

fn process_uid() -> Option<u32> {
    fs::metadata("/proc/self").ok().map(|metadata| metadata.uid())
}

pub fn runtime_dir() -> Option<PathBuf> {
    let base = runtime_base()?;
    let owner = process_uid()?;
    let directory = base.join("mission");
    if fs::symlink_metadata(&directory).is_err() {
        DirBuilder::new().recursive(true).mode(0o700).create(&directory).ok()?;
    }
    accept_dir(&directory, owner)
}

pub fn runtime_subdir(name: &str) -> Option<PathBuf> {
    let directory = runtime_dir()?.join(name);
    let owner = process_uid()?;
    if fs::symlink_metadata(&directory).is_err() {
        DirBuilder::new().mode(0o700).create(&directory).ok()?;
    }
    accept_dir(&directory, owner)
}

pub fn accept_dir(directory: &Path, owner: u32) -> Option<PathBuf> {
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        .open(directory)
        .ok()?;
    let metadata = handle.metadata().ok()?;
    if !metadata.is_dir() || metadata.uid() != owner {
        return None;
    }
    if metadata.mode() & 0o077 != 0 {
        handle.set_permissions(fs::Permissions::from_mode(0o700)).ok()?;
        if handle.metadata().ok()?.mode() & 0o077 != 0 {
            return None;
        }
    }
    Some(directory.to_path_buf())
}

pub fn write_atomically(directory: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = directory.join(format!("{name}.tmp.{}", std::process::id()));
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .and_then(|mut file| file.write_all(bytes));
    let renamed = written.and_then(|()| fs::rename(&temporary, directory.join(name)));
    if renamed.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    renamed
}

#[cfg(test)]
pub mod scratch {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(name: &str) -> Self {
            static COUNT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!("omb-{name}-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("temp dir");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("mode");
            Self(path)
        }

        pub fn owner(&self) -> u32 {
            fs::metadata(&self.0).expect("metadata").uid()
        }
    }

    impl std::ops::Deref for TempDir {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl AsRef<Path> for TempDir {
        fn as_ref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
