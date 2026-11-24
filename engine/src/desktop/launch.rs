use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::paths;

const DESKTOP_ID: &str = "openmausbot";

pub fn desktop_file() -> Option<PathBuf> {
    desktop_in(paths::data_home().as_deref())
}

pub fn desktop_in(data_home: Option<&Path>) -> Option<PathBuf> {
    let name = format!("{DESKTOP_ID}.desktop");
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = data_home {
        candidates.push(home.join("applications").join(&name));
    }
    candidates.push(Path::new("/usr/share/applications").join(&name));
    candidates.into_iter().find(|path| path.is_file())
}

pub fn value_in(file: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(file).ok()?;
    let prefix = format!("{key}=");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(ToString::to_string)
}

pub fn exec_line(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '%' {
            if let Some(next) = chars.peek() {
                if "UuFfDdNnIiCcKk".contains(*next) {
                    chars.next();
                    continue;
                }
            }
        }
        out.push(character);
    }
    out.trim_end().to_string()
}

pub fn argv(exec: &str) -> Option<Vec<String>> {
    let line = exec_line(exec);
    if line.is_empty() {
        return None;
    }
    shlex::split(&line).filter(|argv| !argv.is_empty())
}

pub fn icon() -> Option<String> {
    let file = desktop_file()?;
    value_in(&file, "Icon").filter(|value| !value.is_empty())
}

pub fn installed() -> bool {
    if std::env::var_os("OMB_FORCE_NOT_INSTALLED").is_some_and(|value| value == "1") {
        return false;
    }
    let Some(file) = desktop_file() else { return false };
    let Some(program) = value_in(&file, "Exec").and_then(|exec| argv(&exec)).and_then(|argv| argv.into_iter().next())
    else {
        return false;
    };
    program_exists(&program)
}

fn program_exists(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

pub fn launch() -> bool {
    if !installed() {
        return false;
    }
    if let Some(file) = desktop_file() {
        if let Some(exec) = value_in(&file, "Exec") {
            if let Some(argv) = argv(&exec) {
                return spawn(&argv);
            }
        }
    }
    spawn(&["gtk-launch".to_string(), DESKTOP_ID.to_string()])
}

fn spawn(argv: &[String]) -> bool {
    let Some((program, arguments)) = argv.split_first() else {
        return false;
    };
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    use crate::paths::scratch::TempDir;

    fn desktop(directory: &Path, body: &str) -> PathBuf {
        let file = directory.join("applications").join("openmausbot.desktop");
        fs::create_dir_all(file.parent().expect("parent")).expect("applications");
        fs::write(&file, body).expect("desktop file");
        file
    }

    #[test]
    fn the_exec_line_loses_its_placeholders_and_keeps_its_quotes() {
        assert_eq!(exec_line("/opt/app/openmausbot %U"), "/opt/app/openmausbot");
        assert_eq!(exec_line("/opt/app/openmausbot --flag=%f  "), "/opt/app/openmausbot --flag=");
        assert_eq!(exec_line("app %F %u %d %D %n %N %i %I %c %C %k %K"), "app");
        assert_eq!(exec_line("app \"100%% done\" %U"), "app \"100%% done\"");
        assert_eq!(
            argv("/opt/app/openmausbot --profile \"main profile\" %U").expect("argv"),
            vec!["/opt/app/openmausbot", "--profile", "main profile"]
        );
        assert_eq!(argv("/opt/app/openmausbot \"unclosed %U"), None);
        assert_eq!(argv("%U "), None);
    }

    #[test]
    fn the_desktop_file_is_looked_up_in_order_and_its_keys_are_read() {
        let temp = TempDir::new("lookup");
        assert_eq!(desktop_in(Some(&temp.0)), None, "nothing installed: no file");

        let file = desktop(
            &temp.0,
            "[Desktop Entry]\nType=Application\nName=OpenMausBot\nExec=/opt/app/openmausbot %U\nIcon=/opt/app/icon.svg\n",
        );
        assert_eq!(desktop_in(Some(&temp.0)), Some(file.clone()));
        assert_eq!(value_in(&file, "Icon").as_deref(), Some("/opt/app/icon.svg"));
        assert_eq!(value_in(&file, "Exec").as_deref(), Some("/opt/app/openmausbot %U"));
        assert_eq!(value_in(&file, "Missing"), None);
        assert_eq!(argv(&value_in(&file, "Exec").expect("exec")).expect("argv"), vec!["/opt/app/openmausbot"]);
    }

    #[test]
    fn a_launch_runs_the_program_itself_and_a_missing_one_fails() {
        let temp = TempDir::new("spawn");
        let marker = temp.0.join("ran");
        let probe = temp.0.join("probe");
        fs::write(&probe, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).expect("probe");
        fs::set_permissions(&probe, fs::Permissions::from_mode(0o700)).expect("mode");

        assert!(spawn(&[probe.to_string_lossy().into_owned(), "arg".to_string()]));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(marker.exists(), "the program must really have been started");

        assert!(!spawn(&["/nonexistent/omb-not-a-program".to_string()]));
        assert!(!spawn(&[]));
    }
}
