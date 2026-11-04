use std::path::Path;
use std::time::Duration;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

const BUSY_SQL: &str = "SELECT COUNT(*) FROM assignments WHERE status = 'running'";

const LIVE_SQL: &str = "
SELECT assignee_bot_id, status, json, created_at
FROM assignments
WHERE status NOT IN ('completed', 'failed', 'cancelled')
";

const SETTLED_SQL: &str = "
SELECT assignee_bot_id, status, json, finished_at
FROM assignments
WHERE status IN ('failed', 'cancelled') AND finished_at IS NOT NULL
ORDER BY finished_at DESC
LIMIT 50
";

const BUSY_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Default, Clone)]
struct Decoded {
    quote: Option<String>,
    brief: Option<String>,
    started: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Assignment {
    pub bot_id: Value,
    pub status: String,
    pub quote: Option<String>,
    pub brief: Option<String>,
    pub since: f64,
}

#[derive(Debug, Default, Clone)]
pub struct Assignments {
    pub live: Vec<Assignment>,
    pub settled: Vec<Assignment>,
}

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let connection =
            Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        Ok(Database { connection })
    }

    pub fn data_version(&self) -> rusqlite::Result<i64> {
        self.connection.prepare_cached("PRAGMA data_version")?.query_row([], |row| row.get(0))
    }

    pub fn busy_count(&self) -> rusqlite::Result<i64> {
        self.connection.prepare_cached(BUSY_SQL)?.query_row([], |row| row.get(0))
    }

    pub fn read(&self) -> rusqlite::Result<Assignments> {
        let live = self.collect(LIVE_SQL, Stamp::Created)?;
        let settled = self.collect(SETTLED_SQL, Stamp::Finished)?;
        Ok(Assignments { live, settled })
    }

    fn collect(&self, sql: &str, stamp: Stamp) -> rusqlite::Result<Vec<Assignment>> {
        let mut statement = self.connection.prepare_cached(sql)?;
        let mut rows = statement.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let payload = text(row.get_ref(2)?);
            let decoded = decode(payload.as_deref());
            let stamped = number(row.get_ref(3)?);
            let since = match stamp {
                Stamp::Created => decoded.started.unwrap_or(stamped),
                Stamp::Finished => stamped,
            };
            out.push(Assignment {
                bot_id: match row.get_ref(0)? {
                    ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned().into(),
                    ValueRef::Integer(number) => Value::from(number),
                    ValueRef::Real(number) => Value::from(number),
                    _ => Value::Null,
                },
                status: text(row.get_ref(1)?).unwrap_or_default(),
                quote: decoded.quote,
                brief: decoded.brief,
                since,
            });
        }
        drop(rows);
        drop(statement);
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum Stamp {
    Created,
    Finished,
}

fn decode(payload: Option<&str>) -> Decoded {
    let Some(payload) = payload.filter(|text| !text.is_empty()) else {
        return Decoded::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        return Decoded::default();
    };
    if !value.is_object() {
        return Decoded::default();
    }
    let quote = value.get("origin").and_then(Value::as_object).and_then(|origin| origin.get("quote"));
    Decoded {
        quote: quote.and_then(Value::as_str).map(str::to_owned),
        brief: value.get("brief").and_then(Value::as_str).map(str::to_owned),
        started: value.get("startedAt").and_then(number_of),
    }
}

fn text(value: ValueRef<'_>) -> Option<String> {
    match value {
        ValueRef::Text(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

fn number(value: ValueRef<'_>) -> f64 {
    match value {
        ValueRef::Integer(number) => number as f64,
        ValueRef::Real(number) => number,
        ValueRef::Text(bytes) => std::str::from_utf8(bytes).ok().and_then(|t| t.parse().ok()).unwrap_or(0.0),
        _ => 0.0,
    }
}

fn number_of(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::paths::scratch::TempDir;

    use super::*;

    fn busy_count(path: Option<&Path>) -> Option<i64> {
        Database::open(path?).ok()?.busy_count().ok()
    }

    fn read(path: Option<&Path>) -> Option<Assignments> {
        Database::open(path?).ok()?.read().ok()
    }

    fn scratch(name: &str) -> TempDir {
        TempDir::new(&format!("engine-{name}"))
    }

    fn assignments_table(connection: &Connection) {
        connection.execute_batch(include_str!("../tests/schema.sql")).expect("schema");
    }

    #[test]
    fn a_missing_database_is_not_an_error() {
        let dir = scratch("missing");
        let path = dir.join("runtime.db");
        assert!(busy_count(Some(&path)).is_none());
        assert!(read(Some(&path)).is_none());
        assert!(busy_count(None).is_none());
        assert!(read(None).is_none());
    }

    #[test]
    fn busy_counts_only_running_rows() {
        let dir = scratch("busy");
        let path = dir.join("runtime.db");
        let writer = Connection::open(&path).expect("writer");
        assignments_table(&writer);
        writer
            .execute_batch(
                "INSERT INTO assignments (assignee_bot_id, status, json, created_at)
                 VALUES ('a', 'running', '{}', 1), ('b', 'waiting', '{}', 2), ('c', 'completed', '{}', 3);",
            )
            .expect("rows");
        assert_eq!(busy_count(Some(&path)), Some(1));
    }

    #[test]
    fn rows_carry_their_quote_brief_and_stamp() {
        let dir = scratch("rows");
        let path = dir.join("runtime.db");
        let writer = Connection::open(&path).expect("writer");
        assignments_table(&writer);
        writer
            .execute_batch(
                r#"INSERT INTO assignments (assignee_bot_id, status, json, created_at, finished_at) VALUES
                   ('a', 'running', '{"startedAt": 500, "origin": {"quote": "ship it"}}', 100, NULL),
                   ('b', 'waiting', '{"brief": "fix the bug"}', 200, NULL),
                   ('c', 'running', '{not json', 300, NULL),
                   ('d', 'failed', '{}', 400, 900),
                   ('e', 'completed', '{}', 500, 950);"#,
            )
            .expect("rows");

        let assignments = read(Some(&path)).expect("readable");
        assert_eq!(assignments.live.len(), 3);
        assert_eq!(assignments.live[0].quote.as_deref(), Some("ship it"));
        assert_eq!(assignments.live[0].since, 500.0);
        assert_eq!(assignments.live[1].brief.as_deref(), Some("fix the bug"));
        assert_eq!(assignments.live[1].since, 200.0);
        assert_eq!(assignments.live[2].since, 300.0);
        assert_eq!(assignments.settled.len(), 1);
        assert_eq!(assignments.settled[0].status, "failed");
        assert_eq!(assignments.settled[0].since, 900.0);
    }

    #[test]
    fn six_hundred_reads_leave_the_wal_free_to_checkpoint() {
        let dir = scratch("wal");
        let path = dir.join("runtime.db");
        let writer = Connection::open(&path).expect("writer");
        writer.execute_batch("PRAGMA journal_mode=WAL;").expect("wal");
        writer.execute_batch("PRAGMA wal_autocheckpoint=0;").expect("autocheckpoint");
        assignments_table(&writer);

        let reader = Database::open(&path).expect("reader");
        for turn in 0..600 {
            writer
                .execute(
                    "INSERT INTO assignments (assignee_bot_id, status, json, created_at)
                     VALUES ('a', 'running', '{}', ?1)",
                    [turn],
                )
                .expect("the app's write");
            reader.data_version().expect("data_version");
            assert_eq!(reader.busy_count().expect("busy"), turn + 1);
            assert_eq!(reader.read().expect("read").live.len(), turn as usize + 1);
        }

        let busy: i64 = writer.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0)).expect("checkpoint");
        assert_eq!(busy, 0, "a read left a transaction open: the app could not checkpoint");

        reader.connection.execute_batch("BEGIN DEFERRED;").expect("begin");
        assert_eq!(reader.busy_count().expect("busy inside the transaction"), 600);
        writer
            .execute(
                "INSERT INTO assignments (assignee_bot_id, status, json, created_at) VALUES ('b', 'waiting', '{}', 0)",
                rusqlite::params![],
            )
            .expect("a write the reader has not seen");
        let busy: i64 = writer
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .expect("checkpoint with a reader open");
        assert_ne!(busy, 0, "a held read transaction should block the checkpoint");
        reader.connection.execute_batch("ROLLBACK;").expect("rollback");
        let busy: i64 = writer
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .expect("checkpoint after the rollback");
        assert_eq!(busy, 0, "the checkpoint must go through again once the reader is done");
    }
}
