#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
target="$root/engine/tests/fixtures/team"
broken="$root/engine/tests/fixtures/team-corrupt-live"

rm -rf "$target" "$broken"
mkdir -p "$target/usage"

now_ms="$(date +%s%3N)"

cat >"$target/bots.json" <<'JSON'
[
  {
    "id": "boss",
    "name": "Boss",
    "title": "Chief of Staff",
    "description": "Runs the team",
    "chiefOfStaff": true,
    "section": "General",
    "mascotBody": "blob",
    "color": "blue",
    "mascotExpression": "focused"
  },
  { "id": "nameless", "name": "", "title": "No name at all", "mascotBody": "star" },
  {
    "id": "dev",
    "name": "Dev",
    "title": "Engineer",
    "description": "builds things",
    "department": "Engineering",
    "mascotBody": "not-a-body",
    "color": "not-a-colour",
    "mascotExpression": "deadpan"
  },
  {
    "id": "ops",
    "name": "Ops",
    "title": "Monitor",
    "description": "watches status",
    "chiefOfStaff": false,
    "mascotFaceColor": "black",
    "mascotExpression": "not-a-face"
  },
  { "id": "designer", "name": "Designer", "title": "Design", "description": "creative work", "section": "  Design  " },
  { "id": "ghost", "name": "Ghost" },
  { "id": "titled", "name": "Numbered", "title": 0, "description": null },
  "not-a-bot"
]
JSON

sqlite3 "$target/runtime.db" < "$root/engine/tests/schema.sql"
sqlite3 "$target/runtime.db" <<SQL

INSERT INTO assignments (assignee_bot_id, status, json, created_at, finished_at) VALUES
  ('boss', 'running', '{"startedAt": $((now_ms - 60000)), "origin": {"quote": "  Older   turn  "}}', $((now_ms - 90000)), NULL),
  ('boss', 'running', '{"startedAt": $((now_ms - 10000)), "origin": {"quote": "  Ship   the\tmigration plan  "}}', $((now_ms - 90000)), NULL),
  ('boss', 'waiting', '{"brief": "should be ignored, a running turn wins"}', $((now_ms - 5000)), NULL),
  ('dev', 'waiting', '{"brief": "  Fix   the  bug  "}', $((now_ms - 30000)), NULL),
  ('ghost', 'running', '{"brief": "no start stamp"}', $((now_ms - 3000)), NULL),
  ('ops', 'failed', '{"brief": "too old to keep its badge"}', $((now_ms - 700000)), $((now_ms - 600000))),
  ('ops', 'failed', '{not json', $((now_ms - 400000)), $((now_ms - 200000))),
  ('ops', 'completed', '{"brief": "finished"}', $((now_ms - 400000)), $((now_ms - 350000))),
  ('designer', 'cancelled', '{"brief": "cancelled a moment ago"}', $((now_ms - 200000)), $((now_ms - 120000))),
  ('titled', 'waiting', '{"startedAt": 1.5, "brief": "float stamp"}', $((now_ms - 40000)), NULL),
  ('nameless', 'running', '{}', $((now_ms - 2000)), NULL);
SQL

python3 - "$target/usage" <<'PY'
import json
import sys
from datetime import datetime, timedelta, timezone

usage = sys.argv[1]
now = datetime.now(timezone.utc)
local_today = datetime.now().replace(hour=12, minute=0, second=0, microsecond=0)

def iso(stamp):
    return stamp.isoformat()

def record(**fields):
    return json.dumps(fields, separators=(",", ":"))

def month_name(stamp, step):
    index = stamp.year * 12 + (stamp.month - 1) + step
    return "%04d-%02d.jsonl" % (index // 12, index % 12 + 1)

files = {}

def add(step, line):
    files.setdefault(month_name(now, step), []).append(line)

add(-1, record(at=iso(now - timedelta(days=40)), costUsd=1000, outcome={"status": "ok"}))
add(-1, record(at="2019-01-01T00:00:00Z", costUsd=999, outcome={"status": "ok"}))

add(0, record(at=iso(now).replace("+00:00", "Z"), costUsd=0.1, outcome={"status": "ok"}))
add(0, record(at=iso(now - timedelta(hours=1)), costUsd=0.2, outcome={"status": "error"}))
add(0, record(at=iso(now.astimezone(timezone(timedelta(hours=14)))), costUsd=0.3, outcome={"status": "ok"}))
add(0, record(at=iso(now.astimezone(timezone(timedelta(hours=-14)))), costUsd=0.4))
add(0, record(at=iso(local_today), costUsd=1.5, outcome={"status": "ok"}))
add(0, record(at=iso(now).replace("+00:00", "Z"), costUsd="5", outcome={"status": "ok"}))
add(0, record(at=iso(now).replace("+00:00", "Z"), costUsd=0.05, outcome={"status": "ok"}))
add(0, '{"at": "nope", "costUsd": 9}')
add(0, 'not json at all')
add(0, record(costUsd=2, outcome={"status": "ok"}))
add(0, record(at=iso(now - timedelta(days=1)), costUsd=7, outcome={"status": "ok"}))

add(1, record(at=iso(now).replace("+00:00", "Z"), costUsd=0.5, outcome={"status": "ok"}))
add(1, record(at=iso(now + timedelta(days=1)), costUsd=42, outcome={"status": "ok"}))

for name, lines in files.items():
    with open(f"{usage}/{name}", "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")

with open(f"{usage}/{month_name(now, 0)}", "ab") as handle:
    handle.write(b"\xff\xfe not utf-8\n")
PY

cp -a "$target" "$broken"
sqlite3 "$broken/runtime.db" <<SQL
INSERT INTO assignments (assignee_bot_id, status, json, created_at, finished_at)
VALUES ('ghost', 'running', '{not json at all', $((now_ms - 1000)), NULL);
SQL

echo "make-fixtures: wrote $target and $broken"
ls -1 "$target" "$target/usage"
