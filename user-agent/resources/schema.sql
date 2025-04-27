
-- Enable WAL mode and tuning
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA journal_size_limit = 52428800;  -- ~50MB

-- Filesystem events table
CREATE TABLE IF NOT EXISTS fs_events (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    sensor_guid TEXT,
    op          TEXT    NOT NULL,
    path        TEXT    NOT NULL,
    new_path    TEXT,
    pid         INTEGER,
    exe_path    TEXT,
    size        INTEGER,
    sha256      TEXT,
    result      TEXT
);
CREATE INDEX IF NOT EXISTS idx_fs_events_ts  ON fs_events(ts);
CREATE INDEX IF NOT EXISTS idx_fs_events_pid ON fs_events(pid);

-- Network events table
CREATE TABLE IF NOT EXISTS network_events (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    sensor_guid TEXT,
    direction   TEXT    NOT NULL,
    proto       TEXT    NOT NULL,
    src_ip      TEXT    NOT NULL,
    src_port    INTEGER,
    dst_ip      TEXT    NOT NULL,
    dst_port    INTEGER,
    pid         INTEGER,
    exe_path    TEXT,
    bytes       INTEGER,
    verdict     TEXT,
    rule_id     TEXT
);
CREATE INDEX IF NOT EXISTS idx_net_events_ts  ON network_events(ts);
CREATE INDEX IF NOT EXISTS idx_net_events_pid ON network_events(pid);

-- ETW events table
CREATE TABLE IF NOT EXISTS etw_events (
    id            INTEGER PRIMARY KEY,
    ts            INTEGER NOT NULL,
    sensor_guid   TEXT,
    provider_guid TEXT    NOT NULL,
    event_id      INTEGER NOT NULL,
    level         INTEGER,
    pid           INTEGER,
    tid           INTEGER,
    json_payload  TEXT
);
CREATE INDEX IF NOT EXISTS idx_etw_events_ts         ON etw_events(ts);
CREATE INDEX IF NOT EXISTS idx_etw_events_provider   ON etw_events(provider_guid);
CREATE INDEX IF NOT EXISTS idx_etw_events_event_id   ON etw_events(event_id);

-- Configuration tables (scanner, process, fs, network, etw)
CREATE TABLE IF NOT EXISTS scanner_config (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    enabled          BOOLEAN NOT NULL DEFAULT TRUE,
    interval_seconds INTEGER NOT NULL DEFAULT 600,
    recursive        BOOLEAN NOT NULL DEFAULT TRUE,
    file_extensions  TEXT    NOT NULL DEFAULT '.exe,.dll,.bat',
    paths            TEXT    NOT NULL DEFAULT '[]'
    );

CREATE TABLE IF NOT EXISTS process_config (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    enabled               BOOLEAN NOT NULL DEFAULT TRUE,
    hook_creation         BOOLEAN NOT NULL DEFAULT TRUE,
    hook_termination      BOOLEAN NOT NULL DEFAULT FALSE,
    detect_remote_threads BOOLEAN NOT NULL DEFAULT TRUE
    );

CREATE TABLE IF NOT EXISTS fs_config (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    filter_mask    INTEGER NOT NULL DEFAULT 0x1F,
    path_whitelist TEXT    NOT NULL DEFAULT '[]',
    path_blacklist TEXT    NOT NULL DEFAULT '[]'
    );

CREATE TABLE IF NOT EXISTS network_config (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    inspect_dns    BOOLEAN NOT NULL DEFAULT FALSE,
    include_ports  TEXT    NOT NULL DEFAULT '[]',
    exclude_ports  TEXT    NOT NULL DEFAULT '[]'
    );

CREATE TABLE IF NOT EXISTS etw_config (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    level      INTEGER NOT NULL DEFAULT 4,
    keywords   INTEGER NOT NULL DEFAULT 0xFFFFFFFF,
    providers  TEXT    NOT NULL DEFAULT '[]'
    );

-- Configuration auditing
CREATE TABLE IF NOT EXISTS config_audit (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    sensor_type TEXT    NOT NULL,
    changed_at  INTEGER NOT NULL,
    actor       TEXT    NOT NULL,
    old_config  TEXT    NOT NULL,
    new_config  TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_config_audit_time ON config_audit(changed_at);