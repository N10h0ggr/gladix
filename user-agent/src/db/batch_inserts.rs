// src/db/batch_inserts.rs

use rusqlite::{params, Result as SqlResult, Statement};
use prost_types::Timestamp;

use crate::comms::WrappedEvent;
use shared::events::{
    FileEvent,
    NetworkEvent,
    EtwEvent,
    ProcessEvent,
    network_event::Direction as NetDirection,
};

/// Convierte un prost_types::Timestamp en micros UNIX.
fn timestamp_micros(ts: &Timestamp) -> i64 {
    ts.seconds
        .saturating_mul(1_000_000)
        .saturating_add((ts.nanos as i64) / 1_000)
}

/// Trait para insertar un registro en SQLite.
pub trait BatchInsert<T> {
    /// SQL de inserción para una fila.
    fn insert_sql() -> &'static str;
    /// Vincula los campos de `record` y ejecuta la sentencia.
    fn bind_and_execute(stmt: &mut Statement<'_>, record: &T) -> SqlResult<()>;
}

/// FS EVENTS: WrappedEvent<FileEvent>
impl BatchInsert<WrappedEvent<FileEvent>> for WrappedEvent<FileEvent> {
    fn insert_sql() -> &'static str {
        "INSERT INTO fs_events \
         (ts, sensor_guid, op, path, new_path, pid, exe_path, size, sha256, result) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"
    }

    fn bind_and_execute(stmt: &mut Statement<'_>, rec: &WrappedEvent<FileEvent>) -> SqlResult<()> {
        let ts     = timestamp_micros(&rec.ts);
        let sensor = &rec.sensor_guid;
        let ev     = &rec.payload;

        stmt.execute(params![
            ts,
            sensor,
            format!("{:?}", ev.op),
            &ev.path,
            &ev.new_path,             // ya es String
            ev.pid as i64,
            &ev.exe_path,
            ev.size as i64,
            &ev.sha256,
            ev.success.to_string(),
        ])?;
        Ok(())
    }
}

/// NETWORK EVENTS: WrappedEvent<NetworkEvent>
impl BatchInsert<WrappedEvent<NetworkEvent>> for WrappedEvent<NetworkEvent> {
    fn insert_sql() -> &'static str {
        "INSERT INTO network_events \
         (ts, sensor_guid, direction, proto, src_ip, src_port, \
          dst_ip, dst_port, pid, exe_path, bytes, verdict) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
    }

    fn bind_and_execute(stmt: &mut Statement<'_>, rec: &WrappedEvent<NetworkEvent>) -> SqlResult<()> {
        let ts     = timestamp_micros(&rec.ts);
        let sensor = &rec.sensor_guid;
        let ev     = &rec.payload;

        // Convertimos el i32 a la enum y luego a su nombre
        let dir = NetDirection::try_from(ev.direction)
            .unwrap_or(NetDirection::Inbound)
            .as_str_name();

        stmt.execute(params![
            ts,
            sensor,
            dir,
            &ev.proto,
            &ev.src_ip,
            ev.src_port as i64,
            &ev.dst_ip,
            ev.dst_port as i64,
            ev.pid as i64,
            &ev.exe_path,
            ev.bytes as i64,
            ev.blocked.to_string(),
        ])?;
        Ok(())
    }
}

/// ETW EVENTS: WrappedEvent<EtwEvent>
impl BatchInsert<WrappedEvent<EtwEvent>> for WrappedEvent<EtwEvent> {
    fn insert_sql() -> &'static str {
        "INSERT INTO etw_events \
         (ts, sensor_guid, provider_guid, event_id, level, pid, tid, json_payload) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)"
    }

    fn bind_and_execute(stmt: &mut Statement<'_>, rec: &WrappedEvent<EtwEvent>) -> SqlResult<()> {
        let ts     = timestamp_micros(&rec.ts);
        let sensor = &rec.sensor_guid;
        let ev     = &rec.payload;

        stmt.execute(params![
            ts,
            sensor,
            &ev.provider_guid,
            ev.event_id as i64,
            ev.level as i64,
            ev.pid as i64,
            ev.tid as i64,
            &ev.json_payload,
        ])?;
        Ok(())
    }
}


/// PROCESS EVENTS: WrappedEvent<ProcessEvent>
impl BatchInsert<WrappedEvent<ProcessEvent>> for WrappedEvent<ProcessEvent> {
    fn insert_sql() -> &'static str {
        "INSERT INTO process_events \
         (ts, sensor_guid, pid, ppid, image_path, cmdline) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    }

    fn bind_and_execute(stmt: &mut Statement<'_>, rec: &WrappedEvent<ProcessEvent>) -> SqlResult<()> {
        let ts     = timestamp_micros(&rec.ts);
        let sensor = &rec.sensor_guid;
        let ev     = &rec.payload;

        stmt.execute(params![
            ts,
            sensor,
            ev.pid as i64,
            ev.ppid as i64,
            &ev.image_path,
            &ev.cmdline,
        ])?;
        Ok(())
    }
}
