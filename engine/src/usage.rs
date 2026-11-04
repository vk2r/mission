use std::collections::HashMap;
use std::path::Path;

use jiff::tz::TimeZone;
use jiff::{Timestamp, civil};
use serde_json::{Value, json};

use crate::ledger::{At, Ledger, Rec};

pub type Known = HashMap<String, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    Day,
    Week,
    Month,
}

impl Range {
    pub const ALL: [Range; 3] = [Range::Day, Range::Week, Range::Month];

    pub fn as_str(self) -> &'static str {
        match self {
            Range::Day => "day",
            Range::Week => "week",
            Range::Month => "month",
        }
    }
}

#[cfg(test)]
pub fn range_reports_at(directory: Option<&Path>, now: Timestamp, known: Option<&Known>) -> Value {
    reports_in(&mut Ledger::new(), directory, now, &local_zone(), known)
}

pub fn reports_at(ledger: &mut Ledger, directory: Option<&Path>, now: Timestamp, known: Option<&Known>) -> Value {
    reports_in(ledger, directory, now, &local_zone(), known)
}

impl Range {
    pub fn buckets(self) -> usize {
        match self {
            Range::Day => 24,
            Range::Week => 7,
            Range::Month => 30,
        }
    }
}

pub fn range_bounds(range: Range, today: civil::Date) -> (civil::Date, civil::Date) {
    let back = match range {
        Range::Day => 0,
        Range::Week => 6,
        Range::Month => 29,
    };
    (today.checked_sub(jiff::Span::new().days(back)).unwrap_or(today), today)
}

#[cfg(test)]
fn range_report_in(
    directory: Option<&Path>,
    range: Range,
    now: Timestamp,
    zone: &TimeZone,
    known: Option<&Known>,
) -> Value {
    let mut reports = reports_in(&mut Ledger::new(), directory, now, zone, known);
    reports[range.as_str()].take()
}

fn reports_in(
    ledger: &mut Ledger,
    directory: Option<&Path>,
    now: Timestamp,
    zone: &TimeZone,
    known: Option<&Known>,
) -> Value {
    let today = now.to_zoned(zone.clone()).date();
    let names = watched_files(now);
    if let Some(directory) = directory {
        if let Err(error) = ledger.refresh(directory, &names) {
            let failed = |range: Range| {
                let (from, to) = range_bounds(range, today);
                json!({"ok": false, "range": range.as_str(), "from": from.to_string(), "to": to.to_string(), "error": error.to_string()})
            };
            let mut reports = serde_json::Map::new();
            for range in Range::ALL {
                reports.insert(range.as_str().to_string(), failed(range));
            }
            return Value::Object(reports);
        }
    }

    let mut sums: Vec<Sums> = Range::ALL.iter().map(|range| Sums::new(*range, today)).collect();
    if directory.is_some() {
        for record in ledger.records(&names) {
            let moment = match &record.at {
                At::Instant(instant) => instant.to_zoned(zone.clone()),
                At::Local(stamp) => match local_moment(stamp, zone) {
                    Some(moment) => moment,
                    None => continue,
                },
            };
            for sum in &mut sums {
                sum.add(record, &moment, known);
            }
        }
    }

    let mut reports = serde_json::Map::new();
    for sum in sums {
        reports.insert(sum.range.as_str().to_string(), sum.finish());
    }
    Value::Object(reports)
}

struct Sums {
    range: Range,
    from: civil::Date,
    to: civil::Date,
    cost: f64,
    tasks: u64,
    series: Vec<f64>,
    by_bot: Vec<BotSpend>,
    by_model: Vec<ModelSpend>,
}

struct BotSpend {
    key: String,
    name: String,
    cost: f64,
    series: Vec<f64>,
}

struct ModelSpend {
    name: String,
    cost: f64,
    series: Vec<f64>,
    owners: Vec<BotSpend>,
}

impl Sums {
    fn new(range: Range, today: civil::Date) -> Self {
        let (from, to) = range_bounds(range, today);
        Sums {
            range,
            from,
            to,
            cost: 0.0,
            tasks: 0,
            series: vec![0.0; range.buckets()],
            by_bot: Vec::new(),
            by_model: Vec::new(),
        }
    }

    fn add(&mut self, record: &Rec, moment: &jiff::Zoned, known: Option<&Known>) {
        let local = moment.date();
        if local < self.from || local > self.to {
            return;
        }
        if let Some(value) = record.cost {
            self.cost += value;
            let slot = series_slot(self.range, moment, self.from);
            if let Some(slot) = slot {
                self.series[slot] += value;
            }
            let gone = known.is_some_and(|bots| record.bot_id.as_deref().is_some_and(|id| !bots.contains_key(id)));
            let spender = if gone {
                Some((DELETED_BOTS, DELETED_BOTS))
            } else {
                let name = record.bot_name.as_deref();
                let current = record.bot_id.as_deref().and_then(|id| known?.get(id)).map(String::as_str);
                let current = current.filter(|name| !name.is_empty());
                let key = record.bot_id.as_deref().or(name);
                key.zip(current.or(name))
            };
            if let Some((key, name)) = spender {
                add_to(&mut self.by_bot, key, name, value, Some((slot, self.range.buckets())));
            }
            if let Some(model) = record.model.as_deref() {
                let index = match self.by_model.iter().position(|entry| entry.name == model) {
                    Some(index) => index,
                    None => {
                        self.by_model.push(ModelSpend { name: model.to_string(), cost: 0.0, series: vec![0.0; self.range.buckets()], owners: Vec::new() });
                        self.by_model.len() - 1
                    }
                };
                let entry = &mut self.by_model[index];
                entry.cost += value;
                if let Some(slot) = slot {
                    entry.series[slot] += value;
                }
                if let Some((key, name)) = spender.filter(|(key, _)| *key != DELETED_BOTS) {
                    add_to(&mut entry.owners, key, name, value, None);
                }
            }
        }
        if record.ok {
            self.tasks += 1;
        }
    }

    fn finish(mut self) -> Value {
        let range = self.range;
        self.by_bot.sort_by(|left, right| right.cost.total_cmp(&left.cost).then_with(|| left.name.cmp(&right.name)));
        let mut hidden = 0.0f64;
        let by_bot: Vec<Value> = self
            .by_bot
            .into_iter()
            .filter_map(|spend| {
                if round2(spend.cost) <= 0.0 {
                    hidden += spend.cost;
                    return None;
                }
                Some(if spend.key == DELETED_BOTS {
                    json!({"name": "", "deleted": true, "costUsd": round2(spend.cost), "series": bars(&spend.series)})
                } else {
                    json!({"name": spend.name, "costUsd": round2(spend.cost), "botId": spend.key, "series": bars(&spend.series)})
                })
            })
            .collect();
        self.by_model.sort_by(|left, right| right.cost.total_cmp(&left.cost).then_with(|| left.name.cmp(&right.name)));
        let mut hidden_models = 0.0f64;
        let by_model: Vec<Value> = self
            .by_model
            .into_iter()
            .filter_map(|mut spend| {
                if round2(spend.cost) <= 0.0 {
                    hidden_models += spend.cost;
                    return None;
                }
                spend.owners.sort_by(|left, right| right.cost.total_cmp(&left.cost).then_with(|| left.name.cmp(&right.name)));
                let bars = bars(&spend.series);
                Some(match spend.owners.first() {
                    Some(owner) => json!({"name": spend.name, "costUsd": round2(spend.cost), "bot": owner.name, "botId": owner.key, "series": bars}),
                    None => json!({"name": spend.name, "costUsd": round2(spend.cost), "series": bars}),
                })
            })
            .collect();

        let series: Vec<Value> = series_labels(range, self.from)
            .into_iter()
            .zip(self.series)
            .map(|(label, cost)| json!({"label": label, "costUsd": round6(cost)}))
            .collect();

        json!({
            "ok": true,
            "range": range.as_str(),
            "from": self.from.to_string(),
            "to": self.to.to_string(),
            "costUsd": round2(self.cost),
            "tasks": self.tasks,
            "byBot": by_bot,
            "hiddenUsd": round6(hidden),
            "byModel": by_model,
            "hiddenModelUsd": round6(hidden_models),
            "series": series,
        })
    }
}

fn add_to(rows: &mut Vec<BotSpend>, key: &str, name: &str, value: f64, bars: Option<(Option<usize>, usize)>) {
    let index = match rows.iter().position(|row| row.key == key) {
        Some(index) => index,
        None => {
            let series = vec![0.0; bars.map_or(0, |(_, len)| len)];
            rows.push(BotSpend { key: key.to_string(), name: name.to_string(), cost: 0.0, series });
            rows.len() - 1
        }
    };
    let row = &mut rows[index];
    row.cost += value;
    if row.name != name {
        row.name = name.to_string();
    }
    if let Some(slot) = bars.and_then(|(slot, _)| slot) {
        row.series[slot] += value;
    }
}

const DELETED_BOTS: &str = "\u{0}deleted";

fn series_labels(range: Range, from: civil::Date) -> Vec<String> {
    match range {
        Range::Day => (0..24).map(|hour| format!("{hour:02}")).collect(),
        Range::Week | Range::Month => (0..range.buckets())
            .map(|step| from.checked_add(jiff::Span::new().days(step as i64)).unwrap_or(from).to_string())
            .collect(),
    }
}

fn series_slot(range: Range, moment: &jiff::Zoned, from: civil::Date) -> Option<usize> {
    match range {
        Range::Day => usize::try_from(moment.hour()).ok().filter(|slot| *slot < 24),
        Range::Week | Range::Month => {
            let days = moment.date().since(from).ok()?.get_days();
            usize::try_from(days).ok().filter(|slot| *slot < range.buckets())
        }
    }
}

pub fn bot_name(name: Option<&str>, id: Option<&str>) -> Option<String> {
    name.into_iter().chain(id).find_map(|text| clean(text, NAME_LIMIT))
}

pub fn model_name(raw: &str) -> Option<String> {
    clean(raw, MODEL_LIMIT)
}

pub fn clean(raw: &str, limit: usize) -> Option<String> {
    let mut cut = String::new();
    for character in raw.trim().chars().filter(|character| !is_unsafe(*character)) {
        if cut.len() + character.len_utf8() > limit {
            break;
        }
        cut.push(character);
    }
    let cut = cut.trim();
    (!cut.is_empty()).then(|| cut.to_string())
}

fn is_unsafe(character: char) -> bool {
    character.is_control()
        || matches!(character, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

pub const NAME_LIMIT: usize = 80;

const MODEL_LIMIT: usize = 60;

pub fn watched_files(now: Timestamp) -> Vec<String> {
    let zone = local_zone();
    let today = now.to_zoned(zone).date();
    let earliest = today.checked_sub(jiff::Span::new().days(33)).unwrap_or(today);
    let latest = today.checked_add(jiff::Span::new().days(2)).unwrap_or(today);
    let utc = now.to_zoned(TimeZone::UTC);
    let utc_index = month_index(utc.date());

    let mut indexes: Vec<i32> = (utc_index - 1..=utc_index + 1).collect();
    indexes.extend(month_index(earliest)..=month_index(latest));
    indexes.sort_unstable();
    indexes.dedup();
    indexes.into_iter().map(month_name).collect()
}

fn month_index(date: civil::Date) -> i32 {
    i32::from(date.year()) * 12 + i32::from(date.month()) - 1
}

fn month_name(index: i32) -> String {
    format!("{:04}-{:02}.jsonl", index.div_euclid(12), index.rem_euclid(12) + 1)
}

#[cfg(test)]
pub fn month_files(now: Timestamp) -> Vec<String> {
    let utc = now.to_zoned(TimeZone::UTC);
    let index = month_index(utc.date());
    (-1..=1)
        .map(|step| {
            let month = index + step;
            month_name(month)
        })
        .collect()
}

#[cfg(test)]
fn local_date(stamp: &str, zone: &TimeZone) -> Option<civil::Date> {
    local_moment(stamp, zone).map(|moment| moment.date())
}

fn local_moment(stamp: &str, zone: &TimeZone) -> Option<jiff::Zoned> {
    let stamp = stamp.replace('Z', "+00:00");
    if let Ok(timestamp) = stamp.parse::<Timestamp>() {
        return Some(timestamp.to_zoned(zone.clone()));
    }
    if let Ok(datetime) = stamp.parse::<civil::DateTime>() {
        return datetime.to_zoned(zone.clone()).ok();
    }
    if let Ok(date) = stamp.parse::<civil::Date>() {
        return date.to_zoned(zone.clone()).ok();
    }
    if stamp.as_bytes().get(10) == Some(&b' ') {
        let mut fixed = stamp.clone();
        fixed.replace_range(10..11, "T");
        if let Ok(timestamp) = fixed.parse::<Timestamp>() {
            return Some(timestamp.to_zoned(zone.clone()));
        }
    }
    None
}

pub fn local_zone() -> TimeZone {
    TimeZone::system()
}

fn bars(series: &[f64]) -> Vec<f64> {
    series.iter().map(|cost| round6(*cost)).collect()
}

fn round6(cost: f64) -> f64 {
    (cost * 1e6).round() / 1e6
}

fn round2(cost: f64) -> f64 {
    format!("{cost:.2}").parse().unwrap_or(cost)
}

#[cfg(test)]
mod tests {
    use jiff::tz::TimeZone;

    use super::*;

    #[test]
    fn a_stamp_with_an_offset_is_placed_by_its_own_offset() {
        let tokyo = TimeZone::get("Asia/Tokyo").expect("zone");
        assert_eq!(local_date("2026-10-31T23:30:00Z", &tokyo).map(|date| date.to_string()), Some("2026-11-01".into()));
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        assert_eq!(
            local_date("2026-10-31T23:30:00Z", &santiago).map(|date| date.to_string()),
            Some("2026-10-31".into())
        );
    }

    #[test]
    fn a_stamp_without_an_offset_is_local_time() {
        let tokyo = TimeZone::get("Asia/Tokyo").expect("zone");
        assert_eq!(local_date("2026-10-31T00:30:00", &tokyo).map(|date| date.to_string()), Some("2026-10-31".into()));
        assert_eq!(local_date("2026-10-31", &tokyo).map(|date| date.to_string()), Some("2026-10-31".into()));
        assert_eq!(
            local_date("2026-10-31 12:00:00+02:00", &tokyo).map(|date| date.to_string()),
            Some("2026-10-31".into())
        );
    }

    #[test]
    fn a_stamp_that_names_no_date_is_skipped() {
        let tokyo = TimeZone::get("Asia/Tokyo").expect("zone");
        assert_eq!(local_date("yesterday", &tokyo), None);
        assert_eq!(local_date("", &tokyo), None);
    }

    #[test]
    fn three_month_files_are_named_around_the_utc_month() {
        let names = month_files("2026-01-15T12:00:00Z".parse().expect("stamp"));
        assert_eq!(names, vec!["2025-12.jsonl", "2026-01.jsonl", "2026-02.jsonl"]);
    }

    #[test]
    fn the_cost_is_rounded_to_two_decimals_as_python_did() {
        assert_eq!(round2(24.935), 24.93);
        assert_eq!(round2(0.1 + 0.2), 0.3);
        assert_eq!(round2(2.675), 2.67);
        assert_eq!(round2(0.125), 0.12);
        assert_eq!(round2(0.0), 0.0);
    }

    fn scratch(name: &str, files: &[(&str, Vec<String>)]) -> crate::paths::scratch::TempDir {
        let root = crate::paths::scratch::TempDir::new(&format!("usage-{name}"));
        for (file, lines) in files {
            std::fs::write(root.join(file), lines.join("\n") + "\n").expect("month file");
        }
        root
    }

    fn record_on(at: &str, bot: &str, model: &str, cost: f64, task: bool) -> String {
        format!(
            "{{\"at\":\"{at}\",\"botName\":\"{bot}\",\"model\":\"{model}\",\"costUsd\":{cost}{}}}",
            if task { ",\"outcome\":{\"status\":\"ok\"}" } else { "" }
        )
    }

    fn bare(row: &Value) -> Value {
        let mut row = row.clone();
        row.as_object_mut().expect("row").remove("series");
        row
    }

    fn record(at: &str, bot: &str, cost: f64, task: bool) -> String {
        if task {
            format!("{{\"at\":\"{at}\",\"botName\":\"{bot}\",\"costUsd\":{cost},\"outcome\":{{\"status\":\"ok\"}}}}")
        } else {
            format!("{{\"at\":\"{at}\",\"botName\":\"{bot}\",\"costUsd\":{cost}}}")
        }
    }

    #[test]
    fn a_deleted_bot_is_one_row_and_a_sub_cent_row_is_hidden() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-01T12:00:00-03:00".parse().expect("stamp");
        let line = |id: &str, name: &str, cost: f64| {
            format!("{{\"at\":\"2026-11-01T10:00:00-03:00\",\"botId\":\"{id}\",\"botName\":\"{name}\",\"costUsd\":{cost}}}")
        };
        let directory = scratch(
            "deleted",
            &[(
                "2026-11.jsonl",
                vec![
                    line("b1", "Boss", 2.0),
                    line("g1", "Ivy", 0.0008),
                    line("g2", "Gone", 1.0),
                    line("b2", "Tiny", 0.003),
                ],
            )],
        );
        let known: Known = [("b1", "Boss"), ("b2", "Tiny")].iter().map(|(id, name)| (id.to_string(), name.to_string())).collect();
        let day = range_report_in(Some(&directory), Range::Day, now, &santiago, Some(&known));
        let rows = day["byBot"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "Boss");
        assert_eq!(rows[1]["deleted"], true);
        assert_eq!(rows[1]["costUsd"], 1.0);
        assert!((day["hiddenUsd"].as_f64().expect("hidden") - 0.003).abs() < 1e-9);
        let visible: f64 = rows.iter().map(|row| row["costUsd"].as_f64().expect("cost")).sum();
        let total = day["costUsd"].as_f64().expect("total");
        assert!((visible + day["hiddenUsd"].as_f64().expect("hidden") - total).abs() < 0.006);
        let unknown = range_report_in(Some(&directory), Range::Day, now, &santiago, None);
        assert!(unknown["byBot"].as_array().expect("rows").iter().any(|row| row["name"] == "Ivy" || row["name"] == "Gone"));
    }

    #[test]
    fn a_renamed_bot_is_one_row_two_bots_with_one_name_are_two_and_a_zero_model_is_hidden() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-01T12:00:00-03:00".parse().expect("stamp");
        let line = |id: &str, name: &str, model: &str, cost: f64| {
            format!("{{\"at\":\"2026-11-01T10:00:00-03:00\",\"botId\":\"{id}\",\"botName\":\"{name}\",\"model\":\"{model}\",\"costUsd\":{cost}}}")
        };
        let directory = scratch(
            "rename",
            &[(
                "2026-11.jsonl",
                vec![
                    line("b1", "Programmer", "big", 1.0),
                    line("b1", "Dev", "big", 2.0),
                    line("b2", "Twin", "big", 4.0),
                    line("b3", "Twin", "big", 0.5),
                    line("b1", "Dev", "tiny", 0.001),
                ],
            )],
        );
        let known: Known = [("b1", "Dev"), ("b2", "Twin"), ("b3", "Twin")].iter().map(|(id, name)| (id.to_string(), name.to_string())).collect();
        let day = range_report_in(Some(&directory), Range::Day, now, &santiago, Some(&known));
        let rows: Vec<(String, f64)> = day["byBot"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| (row["name"].as_str().expect("name").to_string(), row["costUsd"].as_f64().expect("cost")))
            .collect();
        assert_eq!(rows, [("Twin".to_string(), 4.0), ("Dev".to_string(), 3.0), ("Twin".to_string(), 0.5)]);
        let models = day["byModel"].as_array().expect("models");
        assert_eq!(models.len(), 1, "the $0.00 model is not listed");
        assert!((day["hiddenModelUsd"].as_f64().expect("hidden") - 0.001).abs() < 1e-9);
    }

    #[test]
    fn a_model_takes_the_bot_that_spent_most_on_it() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-01T12:00:00-03:00".parse().expect("stamp");
        let line = |id: &str, name: &str, model: &str, cost: f64| {
            format!("{{\"at\":\"2026-11-01T10:00:00-03:00\",\"botId\":\"{id}\",\"botName\":\"{name}\",\"model\":\"{model}\",\"costUsd\":{cost}}}")
        };
        let directory = scratch(
            "modelowner",
            &[(
                "2026-11.jsonl",
                vec![
                    line("b1", "Boss", "shared", 1.0),
                    line("b2", "Dev", "shared", 3.0),
                    line("b1", "Boss", "boss-only", 2.0),
                    line("g1", "Gone", "orphan", 5.0),
                ],
            )],
        );
        let known: Known = [("b1", "Boss"), ("b2", "Dev")].iter().map(|(id, name)| (id.to_string(), name.to_string())).collect();
        let day = range_report_in(Some(&directory), Range::Day, now, &santiago, Some(&known));
        let models = day["byModel"].as_array().expect("models");
        assert_eq!(bare(&models[0]), json!({"name": "orphan", "costUsd": 5.0}));
        assert_eq!(bare(&models[1]), json!({"name": "shared", "costUsd": 4.0, "bot": "Dev", "botId": "b2"}));
        assert_eq!(bare(&models[2]), json!({"name": "boss-only", "costUsd": 2.0, "bot": "Boss", "botId": "b1"}));
    }

    #[test]
    fn a_week_is_the_last_seven_days_and_reads_across_the_month_file() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-01T12:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "week",
            &[
                (
                    "2026-10.jsonl",
                    vec![
                        record("2026-10-25T23:00:00-03:00", "Boss", 9.0, true),
                        record("2026-10-26T00:10:00-03:00", "Boss", 1.5, true),
                    ],
                ),
                ("2026-11.jsonl", vec![record("2026-11-01T10:00:00-03:00", "Dev", 2.25, false)]),
            ],
        );

        let week = range_report_in(Some(&directory), Range::Week, now, &santiago, None);
        assert_eq!(week["ok"], json!(true));
        assert_eq!(week["range"], json!("week"));
        assert_eq!(week["from"], json!("2026-10-26"));
        assert_eq!(week["to"], json!("2026-11-01"));
        assert_eq!(week["costUsd"], json!(3.75));
        assert_eq!(week["tasks"], json!(1));

        let day = range_report_in(Some(&directory), Range::Day, now, &santiago, None);
        assert_eq!(day["from"], json!("2026-11-01"));
        assert_eq!(day["costUsd"], json!(2.25));
    }

    #[test]
    fn a_month_is_the_last_thirty_days() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-15T09:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "month",
            &[
                (
                    "2026-10.jsonl",
                    vec![
                        record("2026-10-16T23:59:00-03:00", "Boss", 4.0, true),
                        record("2026-10-17T00:01:00-03:00", "Boss", 7.0, true),
                        record("2026-10-31T23:59:00-03:00", "Boss", 2.0, true),
                    ],
                ),
                ("2026-11.jsonl", vec![record("2026-11-14T20:00:00-03:00", "Dev", 0.5, true)]),
            ],
        );

        let month = range_report_in(Some(&directory), Range::Month, now, &santiago, None);
        assert_eq!(month["from"], json!("2026-10-17"));
        assert_eq!(month["to"], json!("2026-11-15"));
        assert_eq!(month["costUsd"], json!(9.5));
        assert_eq!(month["tasks"], json!(3));
    }

    #[test]
    fn a_month_on_the_first_of_the_month_is_not_the_same_read_as_the_day() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-01T12:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "first-of-month",
            &[
                ("2026-10.jsonl", vec![record("2026-10-20T10:00:00-03:00", "Boss", 6.0, true)]),
                ("2026-11.jsonl", vec![record("2026-11-01T10:00:00-03:00", "Dev", 1.25, true)]),
            ],
        );

        let day = range_report_in(Some(&directory), Range::Day, now, &santiago, None);
        let month = range_report_in(Some(&directory), Range::Month, now, &santiago, None);
        assert_eq!(day["from"], json!("2026-11-01"));
        assert_eq!(day["costUsd"], json!(1.25));
        assert_eq!(month["from"], json!("2026-10-03"));
        assert_eq!(month["costUsd"], json!(7.25));
        assert_eq!(month["tasks"], json!(2));
        assert_ne!(month["costUsd"], day["costUsd"], "the month reaches back into October");
    }

    #[test]
    fn a_monday_is_still_a_whole_week() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-02T12:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "monday",
            &[
                (
                    "2026-10.jsonl",
                    vec![
                        record("2026-10-26T10:00:00-03:00", "Boss", 3.0, true),
                        record("2026-10-27T10:00:00-03:00", "Boss", 2.0, true),
                    ],
                ),
                ("2026-11.jsonl", vec![record("2026-11-02T10:00:00-03:00", "Dev", 0.5, true)]),
            ],
        );

        let week = range_report_in(Some(&directory), Range::Week, now, &santiago, None);
        assert_eq!(week["from"], json!("2026-10-27"));
        assert_eq!(week["to"], json!("2026-11-02"));
        assert_eq!(week["costUsd"], json!(2.5));
        assert_eq!(week["tasks"], json!(2));
    }

    #[test]
    fn each_bots_bars_add_up_to_the_total_bars() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-15T09:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "botseries",
            &[(
                "2026-11.jsonl",
                vec![
                    record("2026-11-15T00:30:00-03:00", "Boss", 0.25, true),
                    record("2026-11-15T00:45:00-03:00", "Dev", 0.5, true),
                    record("2026-11-15T08:00:00-03:00", "Dev", 1.5, true),
                    record("2026-11-14T23:00:00-03:00", "Boss", 2.0, true),
                    record("2026-10-20T12:00:00-03:00", "Boss", 4.0, true),
                ],
            )],
        );

        for range in Range::ALL {
            let report = range_report_in(Some(&directory), range, now, &santiago, None);
            let total = report["series"].as_array().expect("series");
            let bots = report["byBot"].as_array().expect("byBot");
            assert_eq!(bots.len(), 2, "{}", range.as_str());
            for (slot, bar) in total.iter().enumerate() {
                let sum: f64 = bots.iter().map(|bot| bot["series"][slot].as_f64().expect("bar")).sum();
                assert_eq!(round6(sum), bar["costUsd"].as_f64().expect("cost"), "{} slot {slot}", range.as_str());
            }
            for bot in bots {
                assert_eq!(bot["series"].as_array().expect("series").len(), range.buckets());
                let sum: f64 = bot["series"].as_array().expect("series").iter().filter_map(Value::as_f64).sum();
                assert_eq!(round2(sum), bot["costUsd"].as_f64().expect("cost"), "{}", range.as_str());
            }
        }
    }

    #[test]
    fn each_models_bars_add_up_to_the_total_bars() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-15T09:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "modelseries",
            &[(
                "2026-11.jsonl",
                vec![
                    record_on("2026-11-15T00:30:00-03:00", "Boss", "opus", 0.25, true),
                    record_on("2026-11-15T00:45:00-03:00", "Dev", "sonnet", 0.5, true),
                    record_on("2026-11-15T08:00:00-03:00", "Dev", "sonnet", 1.5, true),
                    record_on("2026-11-14T23:00:00-03:00", "Boss", "opus", 2.0, true),
                    record_on("2026-10-20T12:00:00-03:00", "Boss", "haiku", 4.0, true),
                ],
            )],
        );

        for range in Range::ALL {
            let report = range_report_in(Some(&directory), range, now, &santiago, None);
            let total = report["series"].as_array().expect("series");
            let models = report["byModel"].as_array().expect("byModel");
            assert!(models.len() >= 2, "{}", range.as_str());
            for (slot, bar) in total.iter().enumerate() {
                let sum: f64 = models.iter().map(|model| model["series"][slot].as_f64().expect("bar")).sum();
                assert_eq!(round6(sum), bar["costUsd"].as_f64().expect("cost"), "{} slot {slot}", range.as_str());
            }
            for model in models {
                let bars = model["series"].as_array().expect("series");
                assert_eq!(bars.len(), range.buckets());
                let sum: f64 = bars.iter().filter_map(Value::as_f64).sum();
                assert_eq!(round2(sum), model["costUsd"].as_f64().expect("cost"), "{}", range.as_str());
            }
        }
    }

    #[test]
    fn the_bars_add_up_to_the_range_total() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-15T09:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "series",
            &[(
                "2026-11.jsonl",
                vec![
                    record("2026-11-15T00:30:00-03:00", "Boss", 0.25, true),
                    record("2026-11-15T00:45:00-03:00", "Dev", 0.5, true),
                    record("2026-11-15T08:00:00-03:00", "Dev", 1.5, true),
                    record("2026-11-14T23:00:00-03:00", "Boss", 2.0, true),
                    record("2026-10-20T12:00:00-03:00", "Boss", 4.0, true),
                ],
            )],
        );

        for range in Range::ALL {
            let report = range_report_in(Some(&directory), range, now, &santiago, None);
            let series = report["series"].as_array().expect("series");
            assert_eq!(series.len(), range.buckets(), "{}", range.as_str());
            let sum: f64 = series.iter().map(|bar| bar["costUsd"].as_f64().expect("cost")).sum();
            assert_eq!(round2(sum), report["costUsd"].as_f64().expect("total"), "{}", range.as_str());
            assert!(series.iter().all(|bar| bar["label"].is_string()));
        }

        let day = range_report_in(Some(&directory), Range::Day, now, &santiago, None);
        let bars = day["series"].as_array().expect("series");
        assert_eq!(bars.len(), 24);
        assert_eq!(bars[0], json!({"label": "00", "costUsd": 0.75}));
        assert_eq!(bars[8]["costUsd"], json!(1.5));
        assert_eq!(bars[0]["label"], json!("00"));
        assert_eq!(bars[23]["label"], json!("23"));
        assert_eq!(day["costUsd"], json!(2.25));

        let week = range_report_in(Some(&directory), Range::Week, now, &santiago, None);
        let bars = week["series"].as_array().expect("series");
        assert_eq!(bars.len(), 7);
        assert_eq!(bars[0]["label"], json!("2026-11-09"));
        assert_eq!(bars[6]["label"], json!("2026-11-15"));
        assert_eq!(bars[5]["costUsd"], json!(2.0), "14 November");
        assert_eq!(bars[6]["costUsd"], json!(2.25), "today");

        let month = range_report_in(Some(&directory), Range::Month, now, &santiago, None);
        let bars = month["series"].as_array().expect("series");
        assert_eq!(bars.len(), 30);
        assert_eq!(bars[0]["label"], json!("2026-10-17"));
        assert_eq!(bars[29]["label"], json!("2026-11-15"));
        assert_eq!(bars[3]["costUsd"], json!(4.0), "20 October is three days into the window");
    }

    #[test]
    fn a_week_in_tokyo_counts_the_stamp_santiago_still_dates_october() {
        let tokyo = TimeZone::get("Asia/Tokyo").expect("zone");
        let now: Timestamp = "2026-11-01T08:00:00+09:00".parse().expect("stamp");
        let directory = scratch(
            "tokyo",
            &[(
                "2026-10.jsonl",
                vec![record("2026-10-31T23:30:00Z", "Boss", 4.0, true)],
            )],
        );

        let week = range_report_in(Some(&directory), Range::Week, now, &tokyo, None);
        assert_eq!(week["to"], json!("2026-11-01"));
        assert_eq!(week["costUsd"], json!(4.0));
        let day = range_report_in(Some(&directory), Range::Day, now, &tokyo, None);
        assert_eq!(day["from"], json!("2026-11-01"));
        assert_eq!(day["costUsd"], json!(4.0), "a stamp from the UTC day before is still today in Tokyo");
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let late: Timestamp = "2026-10-31T20:30:00-03:00".parse().expect("stamp");
        let other = range_report_in(Some(&directory), Range::Month, late, &santiago, None);
        assert_eq!(other["from"], json!("2026-10-02"), "thirty days counting 31 October");
        assert_eq!(other["costUsd"], json!(4.0));
    }

    #[test]
    fn a_corrupt_line_costs_only_its_line() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-15T09:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "corrupt",
            &[(
                "2026-11.jsonl",
                vec![
                    record("2026-11-10T10:00:00-03:00", "Boss", 1.25, true),
                    "{\"at\":\"2026-11-11T10:00:00-03:00\",\"costUsd\":".to_string(),
                    "\u{0}\u{1} not json at all".to_string(),
                    record("2026-11-12T10:00:00-03:00", "Dev", 0.75, true),
                ],
            )],
        );

        let month = range_report_in(Some(&directory), Range::Month, now, &santiago, None);
        assert_eq!(month["ok"], json!(true));
        assert_eq!(month["costUsd"], json!(2.0));
        assert_eq!(month["tasks"], json!(2));
        assert_eq!(month["byBot"][0]["name"], json!("Boss"));
        assert_eq!(month["byBot"][1]["name"], json!("Dev"));
    }

    #[test]
    fn the_by_bot_split_adds_up_and_puts_the_biggest_first() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-15T09:00:00-03:00".parse().expect("stamp");
        let directory = scratch(
            "bybot",
            &[(
                "2026-11.jsonl",
                vec![
                    record("2026-11-02T10:00:00-03:00", "Dev", 0.3, true),
                    record("2026-11-03T10:00:00-03:00", "Boss", 2.0, true),
                    record("2026-11-04T10:00:00-03:00", "Dev", 0.2, true),
                    "{\"at\":\"2026-11-05T10:00:00-03:00\",\"costUsd\":1.0}".to_string(),
                ],
            )],
        );

        let month = range_report_in(Some(&directory), Range::Month, now, &santiago, None);
        assert_eq!(month["costUsd"], json!(3.5));
        for (row, name, cost) in [(0, "Boss", 2.0), (1, "Dev", 0.5)] {
            let bot = &month["byBot"][row];
            assert_eq!((bot["name"].clone(), bot["costUsd"].clone(), bot["botId"].clone()), (json!(name), json!(cost), json!(name)));
            assert_eq!(bot["series"].as_array().expect("series").len(), 30);
        }
        assert_eq!(month["byBot"].as_array().expect("rows").len(), 2);
    }

    #[test]
    fn the_by_model_split_adds_up_and_cleans_its_names() {
        let santiago = TimeZone::get("America/Santiago").expect("zone");
        let now: Timestamp = "2026-11-15T09:00:00-03:00".parse().expect("stamp");
        let long = format!("provider/{}", "m".repeat(90));
        let directory = scratch(
            "bymodel",
            &[(
                "2026-11.jsonl",
                vec![
                    record_on("2026-11-02T10:00:00-03:00", "Dev", "claude-sonnet-5", 0.3, true),
                    record_on("2026-11-03T10:00:00-03:00", "Boss", "claude-opus-5-5", 2.0, true),
                    record_on("2026-11-04T10:00:00-03:00", "Dev", "claude-sonnet-5", 0.2, true),
                    record_on("2026-11-04T11:00:00-03:00", "Dev", "  claude-\\u0001sonnet-5  ", 0.1, true),
                    record_on("2026-11-04T12:00:00-03:00", "Dev", &long, 0.25, true),
                    "{\"at\":\"2026-11-05T10:00:00-03:00\",\"botName\":\"Dev\",\"costUsd\":0.5}".to_string(),
                ],
            )],
        );

        let month = range_report_in(Some(&directory), Range::Month, now, &santiago, None);
        assert_eq!(month["costUsd"], json!(3.35));
        let models = month["byModel"].as_array().expect("models");
        assert_eq!(bare(&models[0]), json!({"name": "claude-opus-5-5", "costUsd": 2.0, "bot": "Boss", "botId": "Boss"}));
        assert_eq!(bare(&models[1]), json!({"name": "claude-sonnet-5", "costUsd": 0.6, "bot": "Dev", "botId": "Dev"}));
        assert_eq!(models.len(), 3, "{models:?}");
        let cut = models[2]["name"].as_str().expect("name");
        assert!(cut.len() <= MODEL_LIMIT, "{cut:?} is {} bytes", cut.len());
        assert!(cut.starts_with("provider/mmmm"), "{cut:?}");
        let by_bot: f64 = month["byBot"].as_array().expect("bots").iter().map(|row| row["costUsd"].as_f64().expect("cost")).sum();
        let by_model: f64 = models.iter().map(|row| row["costUsd"].as_f64().expect("cost")).sum();
        assert_eq!(round2(by_bot), json!(3.35));
        assert_eq!(round2(by_model), json!(2.85));
        assert_eq!(round2(by_model + 0.5), month["costUsd"].as_f64().expect("total"));
    }

    #[test]
    fn the_daemon_gets_the_three_ranges_and_their_files() {
        let now: Timestamp = "2026-11-01T12:00:00-03:00".parse().expect("stamp");
        let reports = range_reports_at(Some(Path::new("/nonexistent-openmausbot-usage")), now, None);
        for range in Range::ALL {
            assert_eq!(reports[range.as_str()]["ok"], json!(true));
            assert_eq!(reports[range.as_str()]["range"], json!(range.as_str()));
            assert_eq!(reports[range.as_str()]["costUsd"], json!(0.0));
            assert!(reports[range.as_str()]["byBot"].is_array());
        }

        let files = watched_files(now);
        for name in month_files(now) {
            assert!(files.contains(&name), "{name} is missing from {files:?}");
        }
        let mut sorted = files.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, files, "the stamps are read in file order, once each");
        assert!(files.contains(&"2026-10.jsonl".to_string()), "{files:?}");
    }
}

#[cfg(test)]
mod sanitize_tests {
    use super::*;

    #[test]
    fn clean_drops_controls_and_cuts_on_a_character_boundary() {
        assert_eq!(clean("  a\u{0}b\u{202e}c\u{85}d  ", 80).as_deref(), Some("abcd"));
        assert_eq!(clean("\u{7}\u{1b}", 80), None);
        let cut = clean(&"é".repeat(100), 81).expect("text");
        assert_eq!(cut.len(), 80, "a two-byte character is not split");
        assert!(bot_name(Some(&"n".repeat(1000)), None).expect("name").len() <= NAME_LIMIT);
        assert_eq!(bot_name(Some("\u{7}"), Some("id1")).as_deref(), Some("id1"));
    }
}
