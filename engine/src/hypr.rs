use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::{alert, launch};

pub const CLASS: &str = "com.openmausbot.app";
pub const SPECIAL: &str = "openmausbot";
pub const SPECIAL_WS: &str = "special:openmausbot";

#[derive(Debug, Clone, Deserialize)]
pub struct Client {
    #[serde(default)]
    pub address: Option<String>,
    #[serde(rename = "class", default)]
    pub class: Option<String>,
    #[serde(rename = "initialClass", default)]
    pub initial_class: Option<String>,
    #[serde(default)]
    pub workspace: Option<Workspace>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Workspace {
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Monitor {
    #[serde(default)]
    pub focused: Option<bool>,
    #[serde(rename = "activeWorkspace", default)]
    pub active_workspace: Option<Workspace>,
    #[serde(rename = "specialWorkspace", default)]
    pub special_workspace: Option<Workspace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub address: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Visible,
    Hidden,
    Absent,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Visible => "visible",
            State::Hidden => "hidden",
            State::Absent => "absent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Launch,
    Focus { address: String },
    Move { workspace: String, address: String },
    ToggleSpecial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

pub fn clients(json: Option<&str>) -> Vec<Client> {
    entries::<Client>(json)
}

pub fn monitors(json: Option<&str>) -> Vec<Monitor> {
    entries::<Monitor>(json)
}

fn entries<T: for<'de> Deserialize<'de>>(json: Option<&str>) -> Vec<T> {
    let Some(json) = json else {
        return Vec::new();
    };
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect()
}

pub fn window(clients: &[Client]) -> Option<Window> {
    let client = clients
        .iter()
        .find(|client| client.class.as_deref() == Some(CLASS) || client.initial_class.as_deref() == Some(CLASS))?;
    Some(Window {
        address: client.address.clone().unwrap_or_default(),
        workspace: client.workspace.as_ref().and_then(|workspace| workspace.name.clone()).unwrap_or_default(),
    })
}

pub fn special_shown(monitors: &[Monitor]) -> bool {
    monitors.iter().any(|monitor| {
        monitor.special_workspace.as_ref().and_then(|workspace| workspace.name.as_deref()) == Some(SPECIAL_WS)
    })
}

pub fn focused_workspace_id(monitors: &[Monitor]) -> Option<String> {
    let monitor = monitors.iter().find(|monitor| monitor.focused == Some(true))?;
    let id = monitor.active_workspace.as_ref()?.id.as_ref()?;
    match id {
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

pub fn state(window: Option<&Window>, monitors: &[Monitor]) -> State {
    match window {
        None => State::Absent,
        Some(window) if window.address.is_empty() => State::Absent,
        Some(window) if window.workspace == SPECIAL_WS && !special_shown(monitors) => State::Hidden,
        Some(_) => State::Visible,
    }
}

pub fn plan_show(window: Option<&Window>, monitors: &[Monitor]) -> Vec<Step> {
    let Some(window) = present(window) else {
        return vec![Step::Launch];
    };
    if window.workspace != SPECIAL_WS {
        return vec![Step::Focus { address: window.address.clone() }];
    }
    match focused_workspace_id(monitors).as_deref().and_then(positive_id) {
        Some(target) => {
            let mut steps = vec![Step::Move {
                workspace: target.to_string(),
                address: window.address.clone(),
            }];
            if special_shown(monitors) {
                steps.push(Step::ToggleSpecial);
            }
            steps.push(Step::Focus { address: window.address.clone() });
            steps
        }
        None => vec![Step::ToggleSpecial],
    }
}

pub fn plan_hide(window: Option<&Window>, monitors: &[Monitor]) -> Vec<Step> {
    let Some(window) = present(window) else {
        return Vec::new();
    };
    if window.workspace == SPECIAL_WS {
        return if special_shown(monitors) { vec![Step::ToggleSpecial] } else { Vec::new() };
    }
    let mut steps = vec![Step::Move {
        workspace: SPECIAL_WS.to_string(),
        address: window.address.clone(),
    }];
    if special_shown(monitors) {
        steps.push(Step::ToggleSpecial);
    }
    steps
}

pub fn plan_launch(window: Option<&Window>, monitors: &[Monitor]) -> Vec<Step> {
    match state(window, monitors) {
        State::Absent => vec![Step::Launch],
        State::Hidden | State::Visible => Vec::new(),
    }
}

pub fn plan_toggle(window: Option<&Window>, monitors: &[Monitor]) -> Vec<Step> {
    match state(window, monitors) {
        State::Absent => vec![Step::Launch],
        State::Hidden => plan_show(window, monitors),
        State::Visible => plan_hide(window, monitors),
    }
}

fn present(window: Option<&Window>) -> Option<&Window> {
    window.filter(|window| !window.address.is_empty())
}

fn positive_id(text: &str) -> Option<&str> {
    let value: i64 = text.parse().ok()?;
    (value > 0 && !text.starts_with('+') && text.bytes().all(|byte| byte.is_ascii_digit())).then_some(text)
}

fn is_address(text: &str) -> bool {
    let Some(digits) = text.strip_prefix("0x") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl Step {
    pub fn code(&self) -> Result<Option<String>, Refused> {
        match self {
            Step::Launch => Ok(None),
            Step::ToggleSpecial => Ok(Some(format!("hl.dsp.workspace.toggle_special(\"{SPECIAL}\")"))),
            Step::Focus { address } => {
                if !is_address(address) {
                    return Err(Refused);
                }
                Ok(Some(format!("hl.dsp.focus({{ window = \"address:{address}\" }})")))
            }
            Step::Move { workspace, address } => {
                if !is_address(address) || !(workspace == SPECIAL_WS || positive_id(workspace).is_some()) {
                    return Err(Refused);
                }
                Ok(Some(format!(
                    "hl.dsp.window.move({{ workspace = \"{workspace}\", follow = false, window = \"address:{address}\" }})"
                )))
            }
        }
    }
}

pub trait Desktop {
    fn clients_json(&self) -> Option<String>;
    fn monitors_json(&self) -> Option<String>;
    fn dispatch(&self, code: &str) -> bool;
    fn launch(&self) -> bool;
}

pub struct Hyprland;

impl Desktop for Hyprland {
    fn clients_json(&self) -> Option<String> {
        socket_json(b"j/clients").or_else(|| hyprctl_json(&["clients", "-j"]))
    }

    fn monitors_json(&self) -> Option<String> {
        socket_json(b"j/monitors").or_else(|| hyprctl_json(&["monitors", "-j"]))
    }

    fn dispatch(&self, code: &str) -> bool {
        Command::new("hyprctl")
            .arg("dispatch")
            .arg(code)
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn launch(&self) -> bool {
        launch::launch()
    }
}

const SOCKET_TIMEOUT: Duration = Duration::from_millis(500);
const SOCKET_LIMIT: u64 = 4 << 20;

fn request_socket() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|value| !value.is_empty())?;
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let plain = !signature.is_empty()
        && signature.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    plain.then(|| PathBuf::from(runtime).join("hypr").join(signature).join(".socket.sock"))
}

fn socket_json(request: &[u8]) -> Option<String> {
    socket_json_at(&request_socket()?, request)
}

fn socket_json_at(socket: &std::path::Path, request: &[u8]) -> Option<String> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(SOCKET_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(SOCKET_TIMEOUT)).ok()?;
    stream.write_all(request).ok()?;
    let mut answer = Vec::new();
    stream.take(SOCKET_LIMIT).read_to_end(&mut answer).ok()?;
    answer.trim_ascii_start().starts_with(b"[").then(|| String::from_utf8_lossy(&answer).into_owned())
}

fn hyprctl_json(args: &[&str]) -> Option<String> {
    let output = Command::new("hyprctl").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    State,
    Toggle,
    Show,
    Hide,
    Launch,
    Icon,
    Ack,
}

impl Order {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "state" => Some(Order::State),
            "toggle" => Some(Order::Toggle),
            "show" => Some(Order::Show),
            "hide" => Some(Order::Hide),
            "launch" => Some(Order::Launch),
            "icon" => Some(Order::Icon),
            "ack" => Some(Order::Ack),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub exit: i32,
    pub line: Option<String>,
}

impl Report {
    fn quiet(exit: i32) -> Self {
        Report { exit, line: None }
    }
}

pub fn read_for_state(desktop: &(impl Desktop + ?Sized)) -> (Option<Window>, Vec<Monitor>) {
    let window = window(&clients(desktop.clients_json().as_deref()));
    let needs_monitors = window
        .as_ref()
        .is_some_and(|window| !window.address.is_empty() && window.workspace == SPECIAL_WS);
    let monitors = if needs_monitors { monitors(desktop.monitors_json().as_deref()) } else { Vec::new() };
    (window, monitors)
}

pub fn run(order: Order, desktop: &(impl Desktop + ?Sized)) -> Report {
    match order {
        Order::State => {
            let (window, monitors) = read_for_state(desktop);
            let state = state(window.as_ref(), &monitors);
            let address = window.map(|window| window.address).unwrap_or_default();
            Report {
                exit: 0,
                line: Some(format!(
                    "{{\"state\":\"{}\",\"address\":{},\"alert\":{}}}",
                    state.name(),
                    quote(&address),
                    alert::pending()
                )),
            }
        }
        Order::Toggle => {
            alert::ack();
            let (window, monitors) = observe(desktop);
            execute(&plan_toggle(window.as_ref(), &monitors), desktop)
        }
        Order::Show => {
            let (window, monitors) = observe(desktop);
            execute(&plan_show(window.as_ref(), &monitors), desktop)
        }
        Order::Hide => {
            let (window, monitors) = observe(desktop);
            execute(&plan_hide(window.as_ref(), &monitors), desktop)
        }
        Order::Launch => {
            let (window, monitors) = observe(desktop);
            execute(&plan_launch(window.as_ref(), &monitors), desktop)
        }
        Order::Icon => match launch::icon() {
            Some(icon) => Report { exit: 0, line: Some(icon) },
            None => Report::quiet(1),
        },
        Order::Ack => {
            alert::ack();
            Report::quiet(0)
        }
    }
}

fn observe(desktop: &(impl Desktop + ?Sized)) -> (Option<Window>, Vec<Monitor>) {
    let window = window(&clients(desktop.clients_json().as_deref()));
    let parked_or_drawn = window.as_ref().is_some_and(|window| !window.address.is_empty());
    let monitors = if parked_or_drawn { monitors(desktop.monitors_json().as_deref()) } else { Vec::new() };
    (window, monitors)
}

fn execute(steps: &[Step], desktop: &(impl Desktop + ?Sized)) -> Report {
    let mut codes = Vec::with_capacity(steps.len());
    for step in steps {
        match step.code() {
            Ok(code) => codes.push(code),
            Err(Refused) => return Report::quiet(1),
        }
    }
    let mut ok = true;
    for (step, code) in steps.iter().zip(codes) {
        match (step, code) {
            (Step::Launch, _) => ok &= desktop.launch(),
            (_, Some(code)) => {
                desktop.dispatch(&code);
            }
            (_, None) => {}
        }
    }
    Report::quiet(if ok { 0 } else { 1 })
}

fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;

    fn fixture(case: &str, name: &str) -> Option<String> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hypr").join(case).join(name);
        std::fs::read_to_string(path).ok()
    }

    fn observe(case: &str) -> (Option<Window>, Vec<Monitor>) {
        let clients = clients(fixture(case, "clients.json").as_deref());
        let monitors = monitors(fixture(case, "monitors.json").as_deref());
        (window(&clients), monitors)
    }

    fn codes(steps: &[Step]) -> Vec<String> {
        steps
            .iter()
            .map(|step| match step.code() {
                Ok(Some(code)) => code,
                Ok(None) => "<launch>".to_string(),
                Err(Refused) => "<refused>".to_string(),
            })
            .collect()
    }

    fn focus(address: &str) -> String {
        format!("hl.dsp.focus({{ window = \"address:{address}\" }})")
    }

    fn move_to(workspace: &str, address: &str) -> String {
        format!("hl.dsp.window.move({{ workspace = \"{workspace}\", follow = false, window = \"address:{address}\" }})")
    }

    fn toggle() -> String {
        "hl.dsp.workspace.toggle_special(\"openmausbot\")".to_string()
    }

    fn serve(answer: Vec<u8>) -> (std::path::PathBuf, std::thread::JoinHandle<Vec<u8>>) {
        use std::io::{Read, Write};
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let folder = std::env::temp_dir().join(format!("omb-hypr-socket-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&folder).expect("folder");
        let path = folder.join(".socket.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = [0u8; 64];
            let read = stream.read(&mut request).expect("request");
            stream.write_all(&answer).expect("answer");
            request[..read].to_vec()
        });
        (path, handle)
    }

    #[test]
    fn the_socket_answers_what_hyprctl_prints() {
        let saved = fixture("visible", "clients.json").expect("fixture");
        let (path, server) = serve(saved.clone().into_bytes());
        let answer = socket_json_at(&path, b"j/clients");
        assert_eq!(server.join().expect("server"), b"j/clients");
        assert_eq!(answer.as_deref(), Some(saved.as_str()));
        assert_eq!(window(&clients(answer.as_deref())).map(|w| w.address), Some("0x1000003".to_string()));
        std::fs::remove_dir_all(path.parent().expect("folder")).expect("cleanup");
    }

    #[test]
    fn a_socket_answer_that_is_not_a_list_falls_back() {
        let (path, server) = serve(b"unknown request".to_vec());
        assert_eq!(socket_json_at(&path, b"j/clients"), None);
        server.join().expect("server");
        std::fs::remove_dir_all(path.parent().expect("folder")).expect("cleanup");
        assert_eq!(socket_json_at(&path, b"j/clients"), None);
    }

    #[test]
    fn a_visible_window_is_focused_and_hidden_into_the_special() {
        let (window, monitors) = observe("visible");
        let window = window.expect("the app's window is on screen");
        assert_eq!(state(Some(&window), &monitors), State::Visible);

        assert_eq!(codes(&plan_show(Some(&window), &monitors)), vec![focus(&window.address)]);
        assert_eq!(codes(&plan_hide(Some(&window), &monitors)), vec![move_to(SPECIAL_WS, &window.address)]);
        assert_eq!(codes(&plan_toggle(Some(&window), &monitors)), vec![move_to(SPECIAL_WS, &window.address)]);
    }

    #[test]
    fn a_parked_window_with_its_workspace_hidden_is_brought_back_to_the_focused_one() {
        let (window, monitors) = observe("parked-hidden");
        let window = window.expect("a parked window is still a window");
        assert_eq!(window.workspace, SPECIAL_WS);
        assert_eq!(state(Some(&window), &monitors), State::Hidden);

        assert_eq!(codes(&plan_show(Some(&window), &monitors)), vec![
            move_to("2", &window.address),
            focus(&window.address)
        ]);
        assert!(codes(&plan_hide(Some(&window), &monitors)).is_empty());
        assert_eq!(codes(&plan_toggle(Some(&window), &monitors)), vec![
            move_to("2", &window.address),
            focus(&window.address)
        ]);
    }

    #[test]
    fn a_parked_window_whose_workspace_is_drawn_is_visible_and_the_special_is_closed() {
        let (window, monitors) = observe("parked-shown");
        let window = window.expect("a parked window is still a window");
        assert!(special_shown(&monitors), "this case draws the special workspace");
        assert_eq!(state(Some(&window), &monitors), State::Visible);

        assert_eq!(codes(&plan_show(Some(&window), &monitors)), vec![
            move_to("2", &window.address),
            toggle(),
            focus(&window.address)
        ]);
        assert_eq!(codes(&plan_hide(Some(&window), &monitors)), vec![toggle()]);
        assert_eq!(codes(&plan_toggle(Some(&window), &monitors)), vec![toggle()]);
    }

    #[test]
    fn without_a_window_the_app_is_launched_and_nothing_is_hidden() {
        let (window, monitors) = observe("absent");
        assert!(window.is_none(), "this case has no OpenMausBot window");
        assert_eq!(state(window.as_ref(), &monitors), State::Absent);
        assert_eq!(codes(&plan_show(window.as_ref(), &monitors)), vec!["<launch>"]);
        assert!(codes(&plan_hide(window.as_ref(), &monitors)).is_empty());
        assert_eq!(codes(&plan_toggle(window.as_ref(), &monitors)), vec!["<launch>"]);
    }

    #[test]
    fn the_user_inside_another_special_workspace_gets_ours_drawn_on_top() {
        let (window, monitors) = observe("other-special");
        let window = window.expect("a parked window is still a window");
        assert_eq!(state(Some(&window), &monitors), State::Hidden);
        assert_eq!(focused_workspace_id(&monitors).as_deref(), Some("-98"));
        assert_eq!(codes(&plan_show(Some(&window), &monitors)), vec![toggle()]);
        assert_eq!(codes(&plan_toggle(Some(&window), &monitors)), vec![toggle()]);
    }

    #[test]
    fn with_two_monitors_the_window_goes_to_the_focused_one() {
        let (window, monitors) = observe("two-monitors");
        let window = window.expect("a parked window is still a window");
        assert_eq!(monitors.len(), 2);
        assert!(special_shown(&monitors));
        assert_eq!(focused_workspace_id(&monitors).as_deref(), Some("2"));
        assert_eq!(codes(&plan_show(Some(&window), &monitors)), vec![
            move_to("2", &window.address),
            toggle(),
            focus(&window.address)
        ]);
    }

    #[test]
    fn the_fixtures_answer_like_the_script_on_every_case() {
        let cases = ["visible", "parked-hidden", "parked-shown", "absent", "other-special", "two-monitors"];
        for case in cases {
            let (window, monitors) = observe(case);
            assert!(
                fixture(case, "clients.json").is_some() && fixture(case, "monitors.json").is_some(),
                "{case}: both files must exist"
            );
            for steps in [
                plan_show(window.as_ref(), &monitors),
                plan_hide(window.as_ref(), &monitors),
                plan_toggle(window.as_ref(), &monitors),
            ] {
                assert!(
                    steps.iter().all(|step| step.code().is_ok()),
                    "{case}: a plan must never contain a value that is refused"
                );
            }
        }
    }

    struct Fake {
        clients: Option<String>,
        monitors: Option<String>,
        sent: RefCell<Vec<String>>,
        launched: RefCell<u32>,
    }

    impl Fake {
        fn new(clients: &str, monitors: &str) -> Self {
            Fake {
                clients: Some(clients.to_string()),
                monitors: Some(monitors.to_string()),
                sent: RefCell::new(Vec::new()),
                launched: RefCell::new(0),
            }
        }
    }

    impl Desktop for Fake {
        fn clients_json(&self) -> Option<String> {
            self.clients.clone()
        }

        fn monitors_json(&self) -> Option<String> {
            self.monitors.clone()
        }

        fn dispatch(&self, code: &str) -> bool {
            self.sent.borrow_mut().push(code.to_string());
            true
        }

        fn launch(&self) -> bool {
            *self.launched.borrow_mut() += 1;
            true
        }
    }

    fn clients_with(address: &str) -> String {
        format!(
            r#"[{{"address":"0x1000001","class":"helium","initialClass":"helium","workspace":{{"id":1,"name":"1"}}}},
                {{"address":{address},"class":"com.openmausbot.app","initialClass":"com.openmausbot.app",
                  "workspace":{{"id":8,"name":"8"}}}}]"#
        )
    }

    const MONITORS: &str = r#"[{"id":0,"focused":true,"activeWorkspace":{"id":2,"name":"2"},"specialWorkspace":{"id":0,"name":""}},
                              {"id":1,"focused":false,"activeWorkspace":{"id":8,"name":"8"},"specialWorkspace":{"id":0,"name":""}}]"#;

    #[test]
    fn an_address_that_is_not_an_address_is_refused_with_nothing_sent() {
        for address in [
            r#""0xZZ""#,
            r#""0x55;hl.dsp.exit()""#,
            r#""$(touch /tmp/x)""#,
            r#""0x""#,
            r#""0X1000003""#,
            r#""0x1000003 ""#,
        ] {
            let fake = Fake::new(&clients_with(address), MONITORS);
            let report = run(Order::Show, &fake);
            assert_eq!(report.exit, 1, "{address}: the order must fail");
            assert!(fake.sent.borrow().is_empty(), "{address}: nothing may reach hyprctl");

            let fake = Fake::new(&clients_with(address), MONITORS);
            assert_eq!(run(Order::Hide, &fake).exit, 1);
            assert!(fake.sent.borrow().is_empty(), "{address}: nothing may reach hyprctl");
        }
    }

    #[test]
    fn a_workspace_that_is_not_an_id_never_reaches_the_dispatch() {
        for id in [r#""2; hl.dsp.exit()""#, "0", "-3", "2.5", "null"] {
            let monitors = format!(
                r#"[{{"id":0,"focused":true,"activeWorkspace":{{"id":{id},"name":"2"}},"specialWorkspace":{{"id":0,"name":""}}}}]"#
            );
            let parked = r#"[{"address":"0x1000003","class":"com.openmausbot.app","workspace":{"id":-99,"name":"special:openmausbot"}}]"#;
            let fake = Fake::new(parked, &monitors);
            assert_eq!(run(Order::Show, &fake).exit, 0, "{id}: the order still succeeds");
            assert_eq!(fake.sent.borrow().as_slice(), [toggle()], "{id}: only the constant may be sent");
        }

        let step = Step::Move {
            workspace: "8 }) hl.dsp.exit()".to_string(),
            address: "0x1000003".to_string(),
        };
        assert_eq!(step.code(), Err(Refused));
        assert_eq!(
            Step::Move { workspace: SPECIAL_WS.to_string(), address: "0x1000003".to_string() }
                .code()
                .expect("the constant is always safe"),
            Some(move_to(SPECIAL_WS, "0x1000003"))
        );
        assert_eq!(
            Step::Move { workspace: "007".to_string(), address: "0x1000003".to_string() }.code(),
            Ok(Some(move_to("007", "0x1000003")))
        );
        assert_eq!(
            Step::Move { workspace: "0".to_string(), address: "0x1000003".to_string() }.code(),
            Err(Refused)
        );
        assert_eq!(Step::Focus { address: "0x55D7A03EFB70".to_string() }.code(), Err(Refused));
    }

    #[test]
    fn the_state_line_is_the_scripts_line() {
        let visible = r#"[{"address":"0x1000003","class":"com.openmausbot.app","initialClass":"com.openmausbot.app","workspace":{"id":8,"name":"8"}}]"#;
        let fake = Fake::new(visible, MONITORS);
        let report = run(Order::State, &fake);
        assert_eq!(report.exit, 0);
        let line = report.line.expect("state always prints");
        assert!(line.starts_with(r#"{"state":"visible","address":"0x1000003","alert":"#), "{line}");
        assert!(line.ends_with('}'));
        assert!(fake.sent.borrow().is_empty());

        let fake = Fake::new("[]", MONITORS);
        assert_eq!(
            run(Order::State, &fake).line.as_deref().map(|line| line.starts_with(r#"{"state":"absent","address":"","alert":"#)),
            Some(true)
        );
    }

    #[test]
    fn launch_starts_the_app_only_when_it_is_absent() {
        let fake = Fake::new("[]", MONITORS);
        assert_eq!(run(Order::Launch, &fake).exit, 0);
        assert_eq!(*fake.launched.borrow(), 1, "no window: the app is started");
        assert!(fake.sent.borrow().is_empty(), "a launch moves no window");

        let visible = r#"[{"address":"0x1000003","class":"com.openmausbot.app","initialClass":"com.openmausbot.app","workspace":{"id":8,"name":"8"}}]"#;
        let parked = r#"[{"address":"0x1000003","class":"com.openmausbot.app","workspace":{"id":-99,"name":"special:openmausbot"}}]"#;
        for clients in [visible, parked] {
            let fake = Fake::new(clients, MONITORS);
            assert_eq!(run(Order::Launch, &fake).exit, 0);
            assert_eq!(*fake.launched.borrow(), 0, "running: no second copy");
            assert!(fake.sent.borrow().is_empty());
        }
        assert_eq!(Order::parse("launch"), Some(Order::Launch));
    }
}
