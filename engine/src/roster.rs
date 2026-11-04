use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::{Mutex, OnceLock};

use serde::de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::avatars;
use crate::db::{Assignment, Assignments};
use crate::mascot;
use crate::paths;

const SETTLED_GRACE_MS: f64 = 300_000.0;

const GENERAL_DEPARTMENT: &str = "General";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub id: Value,
    pub name: Value,
    pub role: &'static str,
    pub department: String,
    pub status: String,
    pub task: String,
    pub avatar: String,
    #[serde(rename = "avatarEyes")]
    pub avatar_eyes: String,
    #[serde(rename = "eyePivot")]
    pub eye_pivot: Option<[f64; 2]>,
    pub expression: String,
    pub body: String,
    pub colour: String,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Variant {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub avatar: String,
    #[serde(rename = "avatarEyes", skip_serializing_if = "String::is_empty")]
    pub avatar_eyes: String,
    #[serde(rename = "eyePivot", skip_serializing_if = "Option::is_none")]
    pub eye_pivot: Option<[f64; 2]>,
}

const ROSTER_LIMIT: u64 = 8 << 20;

const TASK_LIMIT: usize = 600;

const KEPT: [&str; 11] = [
    "id",
    "name",
    "title",
    "description",
    "chiefOfStaff",
    "section",
    "department",
    "color",
    "mascotBody",
    "mascotExpression",
    "mascotFaceColor",
];

struct Slim(Value);

impl<'de> Deserialize<'de> for Slim {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SlimVisitor)
    }
}

struct SlimVisitor;

impl<'de> Visitor<'de> for SlimVisitor {
    type Value = Slim;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Slim, E> {
        Ok(Slim(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Slim, E> {
        Ok(Slim(Value::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Slim, E> {
        Ok(Slim(Value::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Slim, E> {
        Ok(Slim(Value::from(value)))
    }

    fn visit_str<E>(self, value: &str) -> Result<Slim, E> {
        Ok(Slim(Value::String(value.to_owned())))
    }

    fn visit_unit<E>(self) -> Result<Slim, E> {
        Ok(Slim(Value::Null))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Slim, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<Value>()? {
            items.push(item);
        }
        Ok(Slim(Value::Array(items)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Slim, A::Error> {
        let mut kept = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if KEPT.contains(&key.as_str()) {
                kept.insert(key, map.next_value::<Value>()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(Slim(Value::Object(kept)))
    }
}

pub fn read() -> Option<Vec<Value>> {
    let path = paths::bots_file()?;
    let file = OpenOptions::new().read(true).custom_flags(paths::READ_FLAGS).open(&path).ok()?;
    if !file.metadata().ok()?.is_file() {
        eprintln!("mausbot: bots.json ignored: it is not a regular file");
        return None;
    }
    let mut bytes = Vec::new();
    file.take(ROSTER_LIMIT + 1).read_to_end(&mut bytes).ok()?;
    parse_roster(&bytes)
}

fn parse_roster(bytes: &[u8]) -> Option<Vec<Value>> {
    if bytes.len() as u64 > ROSTER_LIMIT {
        eprintln!("mausbot: bots.json ignored: it is over {ROSTER_LIMIT} bytes");
        return None;
    }
    let roster: Vec<Slim> = serde_json::from_slice(bytes).ok()?;
    Some(roster.into_iter().map(|bot| bot.0).collect())
}

pub fn known(roster: &[Value]) -> Option<crate::usage::Known> {
    let ids: crate::usage::Known = roster
        .iter()
        .filter_map(|bot| {
            let id = bot.get("id").and_then(Value::as_str)?;
            let name = bot.get("name").and_then(Value::as_str).unwrap_or_default();
            Some((id.to_owned(), crate::usage::clean(name, crate::usage::NAME_LIMIT).unwrap_or_default()))
        })
        .collect();
    (!ids.is_empty()).then_some(ids)
}

pub fn rows_for_at(roster: &[Value], assignments: Option<&Assignments>, now_ms: f64, grace: f64) -> Vec<Row> {
    let readable = assignments.is_some();
    let live = assignments.map_or(&[][..], |found| &found.live[..]);
    let settled = assignments.map_or(&[][..], |found| &found.settled[..]);

    let mut rows = Vec::with_capacity(roster.len());
    for bot in roster {
        if !bot.is_object() || !truthy(bot.get("name")) {
            continue;
        }
        let bot_id = bot.get("id").cloned().unwrap_or(Value::Null);
        let same_bot = |row: &&Assignment| row.bot_id == bot_id;
        let doing = pick_live(live.iter().filter(same_bot))
            .or_else(|| pick_settled(settled.iter().filter(same_bot), now_ms, grace));

        let status = if readable { status_for(doing) } else { "unknown".to_string() };
        let look = mascot::bot_look(bot, &status);
        let picture = picture_for(look.as_ref());
        let variants = variants_for(look.as_ref());

        rows.push(Row {
            id: bot_id,
            name: shown_name(bot.get("name")),
            role: if bot.get("chiefOfStaff") == Some(&Value::Bool(true)) { "chief" } else { "specialist" },
            department: department_of(bot),
            status,
            task: task_for(doing),
            avatar: picture.0,
            avatar_eyes: picture.1,
            eye_pivot: picture.2,
            expression: look.as_ref().map(|look| look.expression.clone()).unwrap_or_default(),
            body: look.as_ref().map(|look| look.body.clone()).unwrap_or_default(),
            colour: look.as_ref().map(|look| look.colour.clone()).unwrap_or_default(),
            variants,
        });
    }

    rows.sort_by_key(|row| if row.role == "chief" { 0 } else { 1 });
    rows
}

fn picture_for(look: Option<&mascot::Look>) -> (String, String, Option<[f64; 2]>) {
    let Some(look) = look else {
        return (String::new(), String::new(), None);
    };
    let body = published(look, mascot::Part::Body);
    let eyes = published(look, mascot::Part::Eyes);
    (body, eyes, mascot::eye_pivot(look))
}

fn variants_for(look: Option<&mascot::Look>) -> Vec<Variant> {
    let Some(look) = look else {
        return Vec::new();
    };
    let count = mascot::variant_count(&look.expression);
    if count < 2 {
        return Vec::new();
    }
    (0..count)
        .map(|variant| {
            if variant == 0 {
                return Variant { avatar: String::new(), avatar_eyes: String::new(), eye_pivot: None };
            }
            let drawn = mascot::Look { variant, ..look.clone() };
            let (avatar, avatar_eyes, eye_pivot) = picture_for(Some(&drawn));
            Variant { avatar, avatar_eyes, eye_pivot }
        })
        .collect()
}

fn published(look: &mascot::Look, part: mascot::Part) -> String {
    static DRAWN: OnceLock<Mutex<HashMap<String, (u64, String)>>> = OnceLock::new();
    let key = format!(
        "{}|{}|{}|{}|{}|{}",
        look.body, look.colour, look.expression, look.variant, look.ink, matches!(part, mascot::Part::Eyes)
    );
    let drawn = DRAWN.get_or_init(Default::default);
    let pruned = avatars::pruned();
    let mut cache = drawn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match cache.get(&key) {
        Some((at, url)) if *at == pruned => url.clone(),
        _ => {
            let svg = mascot::avatar_svg(look, part).unwrap_or_default();
            let url = avatars::publish(&svg);
            if !url.is_empty() {
                cache.insert(key, (pruned, url.clone()));
            }
            url
        }
    }
}

pub fn picture_paths(rows: &[Row]) -> std::collections::HashSet<std::path::PathBuf> {
    rows.iter()
        .flat_map(|row| {
            [&row.avatar, &row.avatar_eyes]
                .into_iter()
                .chain(row.variants.iter().flat_map(|variant| [&variant.avatar, &variant.avatar_eyes]))
        })
        .filter(|url| !url.is_empty())
        .map(|url| std::path::PathBuf::from(url.trim_start_matches("file://")))
        .collect()
}

pub fn grace_ms() -> f64 {
    std::env::var("OPENMAUSBOT_SETTLED_GRACE_MS")
        .ok()
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(SETTLED_GRACE_MS)
}

fn pick_live<'a>(rows: impl Iterator<Item = &'a Assignment>) -> Option<&'a Assignment> {
    rows.filter(|row| row.status == "running" || row.status == "waiting").min_by(|left, right| {
        let rank = |row: &Assignment| if row.status == "running" { 0 } else { 1 };
        rank(left)
            .cmp(&rank(right))
            .then_with(|| right.since.partial_cmp(&left.since).unwrap_or(std::cmp::Ordering::Equal))
    })
}

fn pick_settled<'a>(rows: impl Iterator<Item = &'a Assignment>, now_ms: f64, grace: f64) -> Option<&'a Assignment> {
    rows.filter(|row| row.since > now_ms - grace)
        .max_by(|left, right| left.since.partial_cmp(&right.since).unwrap_or(std::cmp::Ordering::Equal))
}

fn status_for(row: Option<&Assignment>) -> String {
    match row {
        None => "idle".to_string(),
        Some(row) => match row.status.as_str() {
            "running" => "working".to_string(),
            "waiting" => "waiting".to_string(),
            other => other.to_string(),
        },
    }
}

fn task_for(row: Option<&Assignment>) -> String {
    let Some(row) = row else {
        return String::new();
    };
    let text = non_blank(row.brief.as_deref()).or(row.quote.as_deref()).unwrap_or("");
    crate::usage::clean(&collapse_whitespace(text), TASK_LIMIT).unwrap_or_default()
}

fn shown_name(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(name)) => Value::String(crate::usage::clean(name, crate::usage::NAME_LIMIT).unwrap_or_default()),
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

fn non_blank(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.trim().is_empty())
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn department_of(bot: &Value) -> String {
    for field in ["section", "department"] {
        if let Some(named) = bot.get(field).and_then(Value::as_str) {
            if let Some(named) = crate::usage::clean(named, crate::usage::NAME_LIMIT) {
                return named;
            }
        }
    }
    GENERAL_DEPARTMENT.to_string()
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|number| number != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(fields)) => !fields.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_roster_over_the_cap_is_unreadable() {
        let big = vec![b' '; ROSTER_LIMIT as usize + 1];
        assert!(parse_roster(&big).is_none());
        assert!(parse_roster(b"[{\"id\":\"a\",\"name\":\"A\"}]").is_some());
    }

    #[test]
    fn a_bot_name_is_cleaned_and_cut_like_a_model() {
        let long = "x".repeat(500);
        let roster = vec![json!({"id": "a", "name": format!("  Bo\u{7}b\u{202e}{long}")})];
        let known = known(&roster).expect("one bot");
        let name = &known["a"];
        assert!(name.len() <= crate::usage::NAME_LIMIT, "cut to the cap: {}", name.len());
        assert!(name.starts_with("Bob"), "control and bidi characters are gone: {name:?}");
        assert!(!name.chars().any(char::is_control));
        assert_eq!(shown_name(Some(&json!("A\u{1b}[31mB"))), json!("A[31mB"));
    }

    #[test]
    fn a_symlinked_roster_is_not_followed() {
        let dir = crate::paths::scratch::TempDir::new("roster-link");
        std::fs::write(dir.join("real.json"), "[{\"id\":\"a\"}]").expect("real");
        std::os::unix::fs::symlink(dir.join("real.json"), dir.join("bots.json")).expect("link");
        let opened = OpenOptions::new().read(true).custom_flags(paths::READ_FLAGS).open(dir.join("bots.json"));
        assert!(opened.is_err(), "O_NOFOLLOW refuses the link");
    }

    #[test]
    fn a_fifo_roster_opens_without_blocking_and_is_not_a_file() {
        let dir = crate::paths::scratch::TempDir::new("roster-fifo");
        assert!(std::process::Command::new("mkfifo").arg(dir.join("bots.json")).status().expect("mkfifo").success());
        let opened = OpenOptions::new().read(true).custom_flags(paths::READ_FLAGS).open(dir.join("bots.json")).expect("open");
        assert!(!opened.metadata().expect("metadata").is_file());
    }

    use serde_json::json;

    use super::*;

    fn assignment(id: &str, status: &str, since: f64) -> Assignment {
        Assignment { bot_id: Value::from(id), status: status.to_string(), quote: None, brief: None, since }
    }

    #[test]
    fn the_card_shows_the_brief_before_the_quote() {
        let mut row = assignment("a", "running", 1.0);
        row.quote = Some("what  VK said".into());
        row.brief = Some(" what\nBoss  asked ".into());
        assert_eq!(task_for(Some(&row)), "what Boss asked");
        row.brief = Some("   ".into());
        assert_eq!(task_for(Some(&row)), "what VK said");
        row.brief = None;
        assert_eq!(task_for(Some(&row)), "what VK said");
        assert_eq!(task_for(None), "");
    }

    #[test]
    fn a_running_turn_wins_over_a_waiting_one() {
        let live = [assignment("a", "waiting", 500.0), assignment("a", "running", 100.0)];
        assert_eq!(pick_live(live.iter()).map(|row| row.status.as_str()), Some("running"));
    }

    #[test]
    fn the_newest_of_two_equal_rows_wins() {
        let live = [assignment("a", "running", 100.0), assignment("a", "running", 900.0)];
        assert_eq!(pick_live(live.iter()).map(|row| row.since), Some(900.0));
    }

    #[test]
    fn a_settled_row_only_counts_inside_the_grace() {
        let settled = [assignment("a", "failed", 100.0), assignment("b", "failed", 900.0)];
        assert_eq!(pick_settled(settled.iter(), 1000.0, 300.0).map(|row| row.bot_id.clone()), Some(Value::from("b")));
        assert!(pick_settled(settled.iter(), 1000.0, 50.0).is_none());
    }

    #[test]
    fn the_chief_comes_first_and_the_rest_keep_their_order() {
        let roster = vec![
            json!({"id": "a", "name": "A"}),
            json!({"id": "b", "name": "B", "chiefOfStaff": true}),
            json!({"id": "c", "name": "C"}),
            json!({"id": "d", "department": "no name"}),
            json!("not a bot"),
        ];
        let rows = rows_for_at(&roster, None, 0.0, 0.0);
        let order: Vec<_> = rows.iter().map(|row| row.id.clone()).collect();
        assert_eq!(order, vec![json!("b"), json!("a"), json!("c")]);
        assert_eq!(rows[0].role, "chief");
        assert!(rows.iter().all(|row| row.status == "unknown"));
    }

    #[test]
    fn department_and_task_follow_the_app() {
        let roster = vec![
            json!({"id": "a", "name": "A", "section": "  Design  "}),
            json!({"id": "b", "name": "B", "department": "Eng"}),
            json!({"id": "c", "name": "C", "section": "   "}),
            json!({"id": "d", "name": "D", "section": "\u{202e}Evil\nTeam"}),
        ];
        let rows = rows_for_at(&roster, None, 0.0, 0.0);
        let by_id = |id: &str| rows.iter().find(|row| row.id == id).expect("row");
        assert_eq!(by_id("a").department, "Design");
        assert_eq!(by_id("b").department, "Eng");
        assert_eq!(by_id("c").department, GENERAL_DEPARTMENT);
        assert_eq!(by_id("d").department, "EvilTeam");
        assert_eq!(collapse_whitespace("  Ship   the \t plan \n"), "Ship the plan");
    }
}
