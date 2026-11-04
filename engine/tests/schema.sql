CREATE TABLE IF NOT EXISTS assignments (
    id INTEGER PRIMARY KEY,
    assignee_bot_id TEXT,
    status TEXT,
    json TEXT,
    created_at INTEGER,
    finished_at INTEGER
);
CREATE INDEX IF NOT EXISTS assignments_status ON assignments(status);
CREATE INDEX IF NOT EXISTS assignments_live ON assignments(assignee_bot_id, status);
