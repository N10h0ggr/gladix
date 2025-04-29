use std::{fs, path::PathBuf, thread::sleep, time::Duration};
use tokio::{runtime::Runtime, sync::mpsc};
use rusqlite::Connection;
use chrono::Utc;
use tempfile::NamedTempFile;

use agent::{
    db::{connection::{init_database, db_path}, spawn_writer},
    config::{load, model::Config as AppConfig},
    comms::events::{FileEvent, FileOperation, NetworkEvent, Direction, EtwEvent},
};

/// Block the current thread for twice the flush interval.
fn wait_for_flush(ms: u64) {
    sleep(Duration::from_millis(ms * 2));
}

/// Returns the project root (where Cargo.toml & config.toml live).
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn file_event_flushed_to_db() {
    let exe_dir = project_root();
    let cfg: AppConfig = load(&exe_dir.join("config.toml")).expect("failed to load config.toml");

    // Unique temporary DB file
    let tmp = NamedTempFile::new().expect("create tmpfile");
    let file_name = tmp.path().file_name().unwrap().to_string_lossy().into_owned();
    drop(tmp);

    // Configure DB
    let mut db_cfg = cfg.database;
    db_cfg.path = file_name.clone();
    db_cfg.purge_on_restart = true;

    // Init DB and writer
    let conn = init_database(&exe_dir, &db_cfg).expect("init_database failed");
    let db_file = db_path(&exe_dir, &db_cfg);
    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel::<FileEvent>(1);
    spawn_writer(&rt, conn, rx, &db_cfg);

    // Send event
    tx.blocking_send(FileEvent {
        ts: Utc::now(),
        sensor_guid: "FILE-EVENT".into(),
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

    wait_for_flush(db_cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2.query_row("SELECT COUNT(*) FROM fs_events", [], |r| r.get(0)).unwrap();
    assert_eq!(cnt, 1, "Expected one fs_events row");

    // Clean up
    fs::remove_file(&db_file).ok();
}

#[test]
fn network_event_flushed_to_db() {
    let exe_dir = project_root();
    let cfg: AppConfig = load(&exe_dir.join("config.toml")).expect("failed to load config.toml");

    let tmp = NamedTempFile::new().expect("create tmpfile");
    let file_name = tmp.path().file_name().unwrap().to_string_lossy().into_owned();
    drop(tmp);

    let mut db_cfg = cfg.database;
    db_cfg.path = file_name.clone();
    db_cfg.purge_on_restart = true;

    let conn = init_database(&exe_dir, &db_cfg).unwrap();
    let db_file = db_path(&exe_dir, &db_cfg);
    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel::<NetworkEvent>(1);
    spawn_writer(&rt, conn, rx, &db_cfg);

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

    wait_for_flush(db_cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2.query_row("SELECT COUNT(*) FROM network_events", [], |r| r.get(0)).unwrap();
    assert_eq!(cnt, 1, "Expected one network_events row");

    fs::remove_file(&db_file).ok();
}

#[test]
fn etw_event_flushed_to_db() {
    let exe_dir = project_root();
    let cfg: AppConfig = load(&exe_dir.join("config.toml")).expect("failed to load config.toml");

    let tmp = NamedTempFile::new().expect("create tmpfile");
    let file_name = tmp.path().file_name().unwrap().to_string_lossy().into_owned();
    drop(tmp);

    let mut db_cfg = cfg.database;
    db_cfg.path = file_name.clone();
    db_cfg.purge_on_restart = true;

    let conn = init_database(&exe_dir, &db_cfg).unwrap();
    let db_file = db_path(&exe_dir, &db_cfg);
    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel::<EtwEvent>(1);
    spawn_writer(&rt, conn, rx, &db_cfg);

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

    wait_for_flush(db_cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2.query_row("SELECT COUNT(*) FROM etw_events", [], |r| r.get(0)).unwrap();
    assert_eq!(cnt, 1, "Expected one etw_events row");

    fs::remove_file(&db_file).ok();
}

#[test]
fn flush_on_close_under_batch_size() {
    let exe_dir = project_root();
    let cfg: AppConfig = load(&exe_dir.join("config.toml")).expect("failed to load config.toml");

    let tmp = NamedTempFile::new().expect("create tmpfile");
    let file_name = tmp.path().file_name().unwrap().to_string_lossy().into_owned();
    drop(tmp);

    let mut db_cfg = cfg.database;
    db_cfg.batch_size = 5;
    db_cfg.flush_interval_ms = 50;
    db_cfg.path = file_name.clone();
    db_cfg.purge_on_restart = true;

    let conn = init_database(&exe_dir, &db_cfg).expect("init_database");
    let db_file = db_path(&exe_dir, &db_cfg);
    let rt = Runtime::new().unwrap();
    let (tx, rx) = mpsc::channel::<NetworkEvent>(1);
    spawn_writer(&rt, conn, rx, &db_cfg);

    for _ in 0..3 {
        tx.blocking_send(NetworkEvent {
            ts: Utc::now(),
            sensor_guid: "BATCH".into(),
            direction: Direction::Inbound,
            proto: "UDP".into(),
            src_ip: "1.2.3.4".into(),
            src_port: 123,
            dst_ip: "5.6.7.8".into(),
            dst_port: 456,
            pid: 42,
            exe_path: "C:\\dummy.exe".into(),
            bytes: 100,
            blocked: false,
        }).unwrap();
    }
    drop(tx);

    wait_for_flush(db_cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2.query_row("SELECT COUNT(*) FROM network_events", [], |r| r.get(0)).unwrap();
    assert_eq!(cnt, 3, "writer must flush remaining <batch events on close");

    fs::remove_file(&db_file).ok();
}
