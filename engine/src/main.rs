#![forbid(unsafe_code)]

mod alert;
mod avatars;
mod db;
mod hypr;
mod launch;
mod ledger;
mod logwatch;
mod mascot;
mod paths;
mod pyfmt;
mod roster;
mod stream;
mod usage;

use std::process::ExitCode;

const USAGE: &str = "\
engine - the MausBot engine

Usage:
  engine                     daemon: one JSON line per change on stdout, orders on stdin
  engine --once              one snapshot line, then exit
  engine --no-notify         daemon without desktop notifications
  engine --help              this text

The daemon is the default: the panel keeps its stdin open and reads one line
per change. `engine --once` prints the same line once, for a terminal.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--help" | "-h" | "help") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(flag) if flag.starts_with('-') => daemon(&args),
        None => daemon(&args),
        Some(other) => {
            eprintln!("engine: unknown command {other}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn daemon(args: &[String]) -> ExitCode {
    let mut options = stream::Options { once: false, notify: true, now: None };
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--once" => options.once = true,
            "--no-notify" => options.notify = false,
            "--now" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("engine: --now needs a timestamp\n\n{USAGE}");
                    return ExitCode::from(2);
                };
                match value.parse::<jiff::Timestamp>() {
                    Ok(at) => options.now = Some(at),
                    Err(error) => {
                        eprintln!("engine: --now is not an RFC 3339 timestamp: {error}");
                        return ExitCode::from(2);
                    }
                }
            }
            other => {
                eprintln!("engine: unknown option {other}\n\n{USAGE}");
                return ExitCode::from(2);
            }
        }
        index += 1;
    }
    ExitCode::from(u8::try_from(stream::run(options)).unwrap_or(1))
}
