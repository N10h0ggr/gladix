// tests/listeners.rs

use tokio::time::timeout;
use std::{
    fs::{File, OpenOptions},
    mem::size_of,
    sync::{Arc, atomic::{AtomicUsize, Ordering}},
    time::{Duration, SystemTime},
};
use std::path::PathBuf;
use tempfile::NamedTempFile;
use memmap2::MmapOptions;
use prost::Message;
use tokio::{runtime::Runtime, sync::{mpsc, broadcast}};
use rusqlite::Connection;
use chrono::Utc;

use agent::config::{load, model::Config as AppConfig};
use agent::db::{connection::{init_database, db_path}, spawn_writer};
use agent::comms::{
    WrappedEvent,
    memory_ring::MemoryRing,
    listeners::{Buses, RingListener, Listener},
};
use shared::events::{BaseEvent, base_event, ProcessEvent, NetworkEvent, network_event::Direction};
/// Simula que un driver escribe **solo** el payload serializado en el ring.
fn push_raw_event(file: &File, buf: &[u8]) {
    let header_bytes = 2 * size_of::<AtomicUsize>();
    let data_and_len = 4 + buf.len();
    let pad = (8 - (data_and_len % 8)) % 8;
    let record_size = data_and_len + pad;
    let buf_size = record_size * 2;
    let total_size = header_bytes + buf_size;

    file.set_len(total_size as u64).unwrap();
    let mut mmap = unsafe { MmapOptions::new().map_mut(file).unwrap() };

    // length prefix + payload
    let off = header_bytes;
    mmap[off..off+4].copy_from_slice(&(buf.len() as u32).to_le_bytes());
    mmap[off+4..off+4+buf.len()].copy_from_slice(buf);

    // tail = record_size, head = 0
    let ptr = mmap.as_mut_ptr() as *mut AtomicUsize;
    let tail_ptr = unsafe { ptr.add(1) };
    unsafe { (*tail_ptr).store(record_size, Ordering::Release) };

    mmap.flush().unwrap();
}

#[tokio::test]
async fn test_process_event_listener_reads_and_forwards() {
    // 1) Ring temporal
    let tmp  = NamedTempFile::new().unwrap();
    let file = OpenOptions::new().read(true).write(true).open(tmp.path()).unwrap();

    // 2) Payload
    let proc = ProcessEvent {
        pid: 42,
        ppid: 1,
        image_path: "C:\\foo.exe".to_string(),
        cmdline: "foo".to_string(),
    };
    let mut buf = Vec::new();
    proc.encode(&mut buf).unwrap();

    // 3) Escribimos solo el payload
    push_raw_event(&file, &buf);

    // 4) Listener
    let ring     = MemoryRing::open(tmp.path()).unwrap();
    let listener = Arc::new(RingListener::new("process", ring, "SENSOR"));

    // 5) Canales de WrappedEvent<ProcessEvent>
    let (db_tx, mut db_rx) = mpsc::channel::<WrappedEvent<ProcessEvent>>(8);
    let (intel_tx, _)      = broadcast::channel::<WrappedEvent<ProcessEvent>>(8);
    let mut intel_rx       = intel_tx.subscribe();
    let buses = Buses::<ProcessEvent> { db_tx, intel_tx };

    listener.spawn(buses);

    // 6) Verificamos que lleguen los wrapped.payload
    let got_db = timeout(Duration::from_secs(1), db_rx.recv())
        .await.expect("timeout waiting for db")
        .expect("db channel closed");
    let got_intel = timeout(Duration::from_secs(1), intel_rx.recv())
        .await.expect("timeout waiting for intel")
        .expect("intel channel closed");

    assert_eq!(got_db.payload, proc);
    assert_eq!(got_intel.payload, proc);
}

#[tokio::test]
async fn test_network_event_listener_reads_and_forwards() {
    let tmp  = NamedTempFile::new().unwrap();
    let file = OpenOptions::new().read(true).write(true).open(tmp.path()).unwrap();

    let net = NetworkEvent {
        direction: Direction::Inbound as i32,
        proto:     "TCP".to_string(),
        src_ip:    "127.0.0.1".to_string(),
        src_port:  1234,
        dst_ip:    "8.8.8.8".to_string(),
        dst_port:  80,
        pid:       100,
        exe_path:  "C:\\net.exe".to_string(),
        bytes:     1024,
        blocked:   false,
    };
    let mut buf = Vec::new();
    net.encode(&mut buf).unwrap();

    push_raw_event(&file, &buf);

    let ring     = MemoryRing::open(tmp.path()).unwrap();
    let listener = Arc::new(RingListener::new("network", ring, "SENSOR"));

    let (db_tx, mut db_rx) = mpsc::channel::<WrappedEvent<NetworkEvent>>(8);
    let (intel_tx, _)      = broadcast::channel::<WrappedEvent<NetworkEvent>>(8);
    let mut intel_rx       = intel_tx.subscribe();
    let buses = Buses::<NetworkEvent> { db_tx, intel_tx };

    listener.spawn(buses);

    let got_db = timeout(Duration::from_secs(1), db_rx.recv())
        .await.expect("timeout waiting for db")
        .expect("db channel closed");
    let got_intel = timeout(Duration::from_secs(1), intel_rx.recv())
        .await.expect("timeout waiting for intel")
        .expect("intel channel closed");

    assert_eq!(got_db.payload, net);
    assert_eq!(got_intel.payload, net);
}

#[test]
fn network_event_listener_to_db_e2e() {
    let exe_dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cfg: AppConfig = load(&exe_dir.join("config.toml"))
        .expect("failed to load config.toml");

    let mut db_cfg = cfg.database.clone();
    let tmp = NamedTempFile::new().unwrap();
    db_cfg.path = tmp.path().file_name().unwrap().to_string_lossy().into_owned();
    db_cfg.purge_on_restart = true;

    let conn    = init_database(&exe_dir, &db_cfg).expect("init database");
    let db_path = db_path(&exe_dir, &db_cfg);

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        // 1) lanzamos writer
        let (db_tx, db_rx) = mpsc::channel::<WrappedEvent<NetworkEvent>>(1);
        spawn_writer(&rt, conn, db_rx, &db_cfg);

        // 2) simulamos driver ring
        let tmp_ring = NamedTempFile::new().unwrap();
        let ring_f = OpenOptions::new().read(true).write(true).open(tmp_ring.path()).unwrap();

        let net = NetworkEvent {
            direction: Direction::Outbound as i32,
            proto:     "TCP".to_string(),
            src_ip:    "10.0.0.1".to_string(),
            src_port:  5555,
            dst_ip:    "8.8.8.8".to_string(),
            dst_port:  53,
            pid:       4242,
            exe_path:  "C:\\Windows\\svchost.exe".to_string(),
            bytes:     2048,
            blocked:   false,
        };
        let mut buf = Vec::new();
        net.encode(&mut buf).unwrap();
        push_raw_event(&ring_f, &buf);

        // 3) lanzamos listener
        let ring = MemoryRing::open(tmp_ring.path()).unwrap();
        let listener = Arc::new(RingListener::new("network", ring, "TEST-NET"));
        let (_intel_tx, _) = broadcast::channel::<WrappedEvent<NetworkEvent>>(8);
        let buses = Buses::<NetworkEvent> { db_tx, intel_tx: _intel_tx };
        listener.spawn(buses);
    });

    // dejamos que corran listener+writer
    std::thread::sleep(Duration::from_millis(db_cfg.flush_interval_ms as u64 + 100));

    let conn2 = Connection::open(&db_path).unwrap();
    let cnt: i64 = conn2
        .query_row("SELECT COUNT(*) FROM network_events", [], |r| r.get(0))
        .unwrap();

    assert_eq!(cnt, 1, "Esperaba un único registro en network_events");


    // ── Dump del contenido para inspección ───────────────────────────────
    let mut stmt = conn2
        .prepare("SELECT ts, sensor_guid, direction, proto, src_ip, src_port, dst_ip, dst_port, pid, exe_path, bytes, verdict FROM network_events")
        .unwrap();

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,  // ts
            row.get::<_, String>(1)?, // sensor_guid
            row.get::<_, String>(2)?, // direction
            row.get::<_, String>(3)?, // proto
            row.get::<_, String>(4)?, // src_ip
            row.get::<_, i64>(5)?,    // src_port
            row.get::<_, String>(6)?, // dst_ip
            row.get::<_, i64>(7)?,    // dst_port
            row.get::<_, i64>(8)?,    // pid
            row.get::<_, String>(9)?, // exe_path
            row.get::<_, i64>(10)?,   // bytes
            row.get::<_, String>(11)?,// verdict
        ))
    }).unwrap();

    println!("\nContenido actual de network_events:");
    for row in rows {
        println!("{:?}", row.unwrap());
    }
}