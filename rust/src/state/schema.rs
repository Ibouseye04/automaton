//! Database schema definitions and migrations.

/// Current schema version.
pub const SCHEMA_VERSION: u32 = 3;

/// Full DDL for the automaton state database.
pub const CREATE_SCHEMA: &str = r#"
-- Schema version tracking
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

-- Key-value store for runtime state
CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Agent turns (inference + tool execution history)
CREATE TABLE IF NOT EXISTS turns (
    id              TEXT PRIMARY KEY,
    turn_number     INTEGER NOT NULL,
    state           TEXT NOT NULL DEFAULT 'running',
    messages_json   TEXT NOT NULL DEFAULT '[]',
    token_usage_json TEXT NOT NULL DEFAULT '{}',
    cost_estimate   REAL NOT NULL DEFAULT 0.0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Individual tool calls within turns
CREATE TABLE IF NOT EXISTS tool_calls (
    id            TEXT PRIMARY KEY,
    turn_id       TEXT NOT NULL REFERENCES turns(id),
    tool_name     TEXT NOT NULL,
    arguments_json TEXT NOT NULL DEFAULT '{}',
    output        TEXT,
    success       INTEGER NOT NULL DEFAULT 1,
    duration_ms   INTEGER,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Heartbeat execution log
CREATE TABLE IF NOT EXISTS heartbeat_entries (
    id          TEXT PRIMARY KEY,
    task_name   TEXT NOT NULL,
    result      TEXT,
    success     INTEGER NOT NULL DEFAULT 1,
    executed_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Financial transactions (credits, USDC)
CREATE TABLE IF NOT EXISTS transactions (
    id          TEXT PRIMARY KEY,
    tx_type     TEXT NOT NULL,
    amount      REAL NOT NULL,
    currency    TEXT NOT NULL DEFAULT 'credits',
    description TEXT,
    balance_after REAL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Self-modification audit log
CREATE TABLE IF NOT EXISTS modifications (
    id          TEXT PRIMARY KEY,
    mod_type    TEXT NOT NULL,
    description TEXT NOT NULL,
    file_path   TEXT,
    diff        TEXT,
    reversible  INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Registered skills
CREATE TABLE IF NOT EXISTS skills (
    name          TEXT PRIMARY KEY,
    description   TEXT NOT NULL,
    version       TEXT NOT NULL DEFAULT '1.0.0',
    auto_activate INTEGER NOT NULL DEFAULT 0,
    instructions  TEXT NOT NULL,
    file_path     TEXT,
    loaded_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Spawned children
CREATE TABLE IF NOT EXISTS children (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    sandbox_id      TEXT NOT NULL,
    wallet_address  TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active',
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- On-chain registry records
CREATE TABLE IF NOT EXISTS registry (
    wallet_address TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    metadata_uri   TEXT,
    parent_agent   TEXT,
    token_id       TEXT,
    registered_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Social inbox messages
CREATE TABLE IF NOT EXISTS inbox (
    id            TEXT PRIMARY KEY,
    from_address  TEXT NOT NULL,
    to_address    TEXT NOT NULL,
    content       TEXT NOT NULL,
    read          INTEGER NOT NULL DEFAULT 0,
    timestamp     TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Upstream sync tracking
CREATE TABLE IF NOT EXISTS upstream_commits (
    commit_hash TEXT PRIMARY KEY,
    message     TEXT,
    applied     INTEGER NOT NULL DEFAULT 0,
    reviewed    INTEGER NOT NULL DEFAULT 0,
    fetched_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_turns_created ON turns(created_at);
CREATE INDEX IF NOT EXISTS idx_tool_calls_turn ON tool_calls(turn_id);
CREATE INDEX IF NOT EXISTS idx_heartbeat_task ON heartbeat_entries(task_name);
CREATE INDEX IF NOT EXISTS idx_inbox_read ON inbox(read);
CREATE INDEX IF NOT EXISTS idx_inbox_to ON inbox(to_address);
CREATE INDEX IF NOT EXISTS idx_transactions_created ON transactions(created_at);
CREATE INDEX IF NOT EXISTS idx_modifications_created ON modifications(created_at);
"#;

/// Migration from version 1 to version 2.
pub const MIGRATE_V1_TO_V2: &str = r#"
ALTER TABLE turns ADD COLUMN state TEXT NOT NULL DEFAULT 'running';
"#;

/// Migration from version 2 to version 3.
pub const MIGRATE_V2_TO_V3: &str = r#"
CREATE TABLE IF NOT EXISTS upstream_commits (
    commit_hash TEXT PRIMARY KEY,
    message     TEXT,
    applied     INTEGER NOT NULL DEFAULT 0,
    reviewed    INTEGER NOT NULL DEFAULT 0,
    fetched_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;
