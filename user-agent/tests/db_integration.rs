use std::{path::PathBuf, thread::sleep, time::Duration};
use std::time::SystemTime;
use tokio::{runtime::Runtime, sync::mpsc};
use rusqlite::Connection;
use chrono::Utc;
use tempfile::NamedTempFile;
use shared::events::{FileEvent, file_event::Operation as FileOperation, NetworkEvent, network_event::Direction, EtwEvent};

use agent::{
    db::{connection::{init_database, db_path}, spawn_writer},
    config::{load, model::Config as AppConfig},
    comms::WrappedEvent,
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
    let cfg: AppConfig = load(&exe_dir.join("config.toml"))
        .expect("failed to load config.toml");

    // Base de datos en archivo temporal
    let tmp = NamedTempFile::new().expect("create tmpfile");
    let file_name = tmp.path()
        .file_name().unwrap()
        .to_string_lossy()
        .into_owned();

    let mut db_cfg = cfg.database.clone();
    db_cfg.path = file_name;
    db_cfg.purge_on_restart = true;

    let conn    = init_database(&exe_dir, &db_cfg).expect("init_database failed");
    let db_file = db_path(&exe_dir, &db_cfg);
    let rt      = Runtime::new().unwrap();

    // Canal de WrappedEvent<FileEvent>
    let (tx, rx) = mpsc::channel::<WrappedEvent<FileEvent>>(1);
    spawn_writer(&rt, conn, rx, &db_cfg);

    // Enviamos un solo evento envuelto
    let payload = FileEvent {
        op:       FileOperation::Create as i32,
        path:     "C:\\temp\\a.txt".to_string(),
        new_path: "".to_string(),
        pid:      1234,
        exe_path: "C:\\Windows\\notepad.exe".to_string(),
        size:     42,
        sha256:   b"deadbeef".to_vec(),
        success:  true,
    };
    let wrapped = WrappedEvent {
        ts:          SystemTime::now().into(),
        sensor_guid: "FILE-EVENT".to_string(),
        payload,
    };
    tx.blocking_send(wrapped).unwrap();
    drop(tx);

    wait_for_flush(db_cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2
        .query_row("SELECT COUNT(*) FROM fs_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 1, "Expected one fs_events row");
}

#[test]
fn network_event_flushed_to_db() {
    let exe_dir = project_root();
    let cfg: AppConfig = load(&exe_dir.join("config.toml"))
        .expect("failed to load config.toml");

    let tmp = NamedTempFile::new().expect("create tmpfile");
    let file_name = tmp.path()
        .file_name().unwrap()
        .to_string_lossy()
        .into_owned();

    let mut db_cfg = cfg.database.clone();
    db_cfg.path = file_name;
    db_cfg.purge_on_restart = true;

    let conn    = init_database(&exe_dir, &db_cfg).unwrap();
    let db_file = db_path(&exe_dir, &db_cfg);
    let rt      = Runtime::new().unwrap();

    let (tx, rx) = mpsc::channel::<WrappedEvent<NetworkEvent>>(1);
    spawn_writer(&rt, conn, rx, &db_cfg);

    let payload = NetworkEvent {
        direction: Direction::Outbound as i32,
        proto:     "TCP".to_string(),
        src_ip:    "127.0.0.1".to_string(),
        src_port:  1111,
        dst_ip:    "8.8.8.8".to_string(),
        dst_port:  53,
        pid:       5678,
        exe_path:  "C:\\Windows\\svchost.exe".to_string(),
        bytes:     128,
        blocked:   false,
    };
    let wrapped = WrappedEvent {
        ts:          SystemTime::now().into(),
        sensor_guid: "TEST-NET".to_string(),
        payload,
    };
    tx.blocking_send(wrapped).unwrap();
    drop(tx);

    wait_for_flush(db_cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2
        .query_row("SELECT COUNT(*) FROM network_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 1, "Expected one network_events row");
}

#[test]
fn etw_event_flushed_to_db() {
    let exe_dir = project_root();
    let cfg: AppConfig = load(&exe_dir.join("config.toml"))
        .expect("failed to load config.toml");

    let tmp = NamedTempFile::new().expect("create tmpfile");
    let file_name = tmp.path()
        .file_name().unwrap()
        .to_string_lossy()
        .into_owned();

    let mut db_cfg = cfg.database.clone();
    db_cfg.path = file_name;
    db_cfg.purge_on_restart = true;

    let conn    = init_database(&exe_dir, &db_cfg).unwrap();
    let db_file = db_path(&exe_dir, &db_cfg);
    let rt      = Runtime::new().unwrap();

    let (tx, rx) = mpsc::channel::<WrappedEvent<EtwEvent>>(1);
    spawn_writer(&rt, conn, rx, &db_cfg);

    let payload = EtwEvent {
        provider_guid: "PROV-GUID".to_string(),
        event_id:      42,
        level:         4,
        pid:           9999,
        tid:           8888,
        json_payload:  r#"{"foo":"bar"}"#.to_string(),
    };
    let wrapped = WrappedEvent {
        ts:          SystemTime::now().into(),
        sensor_guid: "TEST-ETW".to_string(),
        payload,
    };
    tx.blocking_send(wrapped).unwrap();
    drop(tx);

    wait_for_flush(db_cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2
        .query_row("SELECT COUNT(*) FROM etw_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 1, "Expected one etw_events row");
}

#[test]
fn flush_on_close_under_batch_size() {
    let exe_dir = project_root();
    let cfg: AppConfig = load(&exe_dir.join("config.toml"))
        .expect("failed to load config.toml");

    let tmp = NamedTempFile::new().expect("create tmpfile");
    let file_name = tmp.path()
        .file_name().unwrap()
        .to_string_lossy()
        .into_owned();

    let mut db_cfg = cfg.database.clone();
    db_cfg.batch_size = 5;
    db_cfg.flush_interval_ms = 50;
    db_cfg.path = file_name;
    db_cfg.purge_on_restart = true;

    let conn    = init_database(&exe_dir, &db_cfg).expect("init_database");
    let db_file = db_path(&exe_dir, &db_cfg);
    let rt      = Runtime::new().unwrap();

    let (tx, rx) = mpsc::channel::<WrappedEvent<NetworkEvent>>(1);
    spawn_writer(&rt, conn, rx, &db_cfg);

    for _ in 0..3 {
        let payload = NetworkEvent {
            direction: Direction::Inbound as i32,
            proto:     "UDP".to_string(),
            src_ip:    "1.2.3.4".to_string(),
            src_port:  123,
            dst_ip:    "5.6.7.8".to_string(),
            dst_port:  456,
            pid:       42,
            exe_path:  "C:\\dummy.exe".to_string(),
            bytes:     100,
            blocked:   false,
        };
        let wrapped = WrappedEvent {
            ts:          SystemTime::now().into(),
            sensor_guid: "BATCH".to_string(),
            payload,
        };
        tx.blocking_send(wrapped.clone()).unwrap();
    }
    drop(tx);

    wait_for_flush(db_cfg.flush_interval_ms);
    let conn2 = Connection::open(&db_file).unwrap();
    let cnt: i64 = conn2
        .query_row("SELECT COUNT(*) FROM network_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 3, "writer must flush remaining <batch events on close");
}