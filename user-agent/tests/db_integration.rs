// tests/db_integration.rs

use std::{path::PathBuf, time::Duration, thread::sleep, fs};
use tokio::{runtime::Runtime, sync::mpsc};
use rusqlite::Connection;
use chrono::Utc;

use agent::{
    db::{connection::{init_database, db_path}, spawn_writer},
    config::types::DatabaseConfig,
    comms::events::{FileEvent, FileOperation, NetworkEvent, Direction, EtwEvent},
};

/// Build a DatabaseConfig that writes to `telemetry.db` under `target/debug`.
fn make_db_config() -> DatabaseConfig {
    let mut cfg = DatabaseConfig::default();
    cfg.purge_on_restart = true;
    cfg
}

/// Block the current thread for twice the flush interval.
fn wait_for_flush(ms: u64) {
    sleep(Duration::from_millis(ms * 2));
}

/// Returns `.../user-agent/target/debug` as exe directory.
fn target_debug_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dbg = manifest.join("target").join("debug");
    fs::create_dir_all(&dbg).expect("Failed to create target/debug");
    dbg
}

#[test]
fn file_event_flushed_to_db() {
    // 1. Use target/debug as our "exe_dir"
    let exe_dir = target_debug_dir();
    let cfg = make_db_config();

    // 2. Init DB (will purge any existing telemetry.db in target/debug)
    let conn = init_database(&exe_dir, &cfg).expect("init_database failed");
    let db_file = db_path(&exe_dir, &cfg);
    println!("Using telemetry.db at {:?}", db_file);

    // 3. Spawn writer
    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel::<FileEvent>(1);
    spawn_writer(&rt, conn, rx, &cfg);

    // 4. Send one FileEvent
    tx.blocking_send(FileEvent {
        ts: Utc::now(),
        sensor_guid: "TEST-GUID".into(),
        op: FileOperation::Create,
        path: "C:\\temp\\a.txt".into(),
        new_path: None,
        pid: 1234,
        exe_path: "C:\\Windows\\notepad.exe".into(),
        size: 42,
        sha256: "deadbeef".into(),
        success: true,
    }).unwrap();

    tx.blocking_send(FileEvent {
        ts: Utc::now(),
        sensor_guid: "TEST-GUID2".into(),
        op: FileOperation::Create,
        path: "C:\\temp\\a.txt".into(),
        new_path: None,
        pid: 1234,
        exe_path: "C:\\Windows\\notepad.exe".into(),
        size: 42,
        sha256: "deadbeef".into(),
        success: true,
    }).unwrap();
    drop(tx);

    // 5. Wait, then verify
    wait_for_flush(cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2
        .query_row("SELECT COUNT(*) FROM fs_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 2, "Expected one fs_events row");
}

#[test]
fn network_event_flushed_to_db() {
    let exe_dir = target_debug_dir();
    let cfg = make_db_config();

    let conn = init_database(&exe_dir, &cfg).unwrap();
    let db_file = db_path(&exe_dir, &cfg);
    println!("Using telemetry.db at {:?}", db_file);

    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel::<NetworkEvent>(10_000);
    spawn_writer(&rt, conn, rx, &cfg);

    tx.blocking_send(NetworkEvent {
        ts: Utc::now(),
        sensor_guid: "TEST-NET".into(),
        direction: Direction::Outbound,
        proto: "TCP".into(),
        src_ip: "127.0.0.1".into(),
        src_port: 1111,
        dst_ip: "8.8.8.8".into(),
        dst_port: 53,
        pid: 5678,
        exe_path: "C:\\Windows\\svchost.exe".into(),
        bytes: 128,
        blocked: false,
    }).unwrap();
    drop(tx);

    wait_for_flush(cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2
        .query_row("SELECT COUNT(*) FROM network_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 1, "Expected one network_events row");
}

#[test]
fn etw_event_flushed_to_db() {
    let exe_dir = target_debug_dir();
    let cfg = make_db_config();

    let conn = init_database(&exe_dir, &cfg).unwrap();
    let db_file = db_path(&exe_dir, &cfg);
    println!("Using telemetry.db at {:?}", db_file);

    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel::<EtwEvent>(10_000);
    spawn_writer(&rt, conn, rx, &cfg);

    tx.blocking_send(EtwEvent {
        ts: Utc::now(),
        sensor_guid: "TEST-ETW".into(),
        provider_guid: "PROV-GUID".into(),
        event_id: 42,
        level: 4,
        pid: 9999,
        tid: 8888,
        json_payload: r#"{"foo":"bar"}"#.into(),
    }).unwrap();
    drop(tx);

    wait_for_flush(cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2
        .query_row("SELECT COUNT(*) FROM etw_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 1, "Expected one etw_events row");
}
