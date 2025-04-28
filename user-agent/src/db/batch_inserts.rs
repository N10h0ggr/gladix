// src/db/batch_inserts.rs

use rusqlite::{params, Result as SqlResult, Statement};
use crate::comms::events::{FileEvent, NetworkEvent, EtwEvent};

pub trait BatchInsert<T> {
    fn insert_sql() -> &'static str;
    fn bind_and_execute(stmt: &mut Statement<'_>, record: &T) -> SqlResult<()>;
}

/// FILE EVENTS
impl BatchInsert<FileEvent> for FileEvent {
    fn insert_sql() -> &'static str {
        "INSERT INTO fs_events \
           (ts, sensor_guid, op, path, new_path, pid, exe_path, size, sha256, result) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"
    }

    fn bind_and_execute(stmt: &mut Statement<'_>, rec: &FileEvent) -> SqlResult<()> {
        stmt.execute(params![
            rec.ts.timestamp_micros(),
            rec.sensor_guid,
            format!("{:?}", rec.op),
            rec.path,
            rec.new_path.clone().unwrap_or_default(),
            rec.pid as i64,
            rec.exe_path,
            rec.size as i64,
            &rec.sha256[..],
            rec.success.to_string(),
        ])?;
        Ok(())
    }
}

/// NETWORK EVENTS
impl BatchInsert<NetworkEvent> for NetworkEvent {
    fn insert_sql() -> &'static str {
        "INSERT INTO network_events \
           (ts,sensor_guid,direction,proto,src_ip,src_port, \
            dst_ip,dst_port,pid,exe_path,bytes,verdict) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
    }

    fn bind_and_execute(stmt: &mut Statement<'_>, rec: &NetworkEvent) -> SqlResult<()> {
        stmt.execute(params![
            rec.ts.timestamp_micros(),
            rec.sensor_guid,
            format!("{:?}", rec.direction),
            rec.proto,
            rec.src_ip,
            rec.src_port as i64,
            rec.dst_ip,
            rec.dst_port as i64,
            rec.pid as i64,
            rec.exe_path,
            rec.bytes as i64,
            rec.blocked.to_string(),
        ])?;
        Ok(())
    }
}

/// ETW EVENTS
impl BatchInsert<EtwEvent> for EtwEvent {
    fn insert_sql() -> &'static str {
        "INSERT INTO etw_events \
           (ts,sensor_guid,provider_guid,event_id,level,pid,tid,json_payload) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)"
    }

    fn bind_and_execute(stmt: &mut Statement<'_>, rec: &EtwEvent) -> SqlResult<()> {
        stmt.execute(params![
            rec.ts.timestamp_micros(),
            rec.sensor_guid,
            rec.provider_guid,
            rec.event_id as i64,
            rec.level as i64,
            rec.pid as i64,
            rec.tid as i64,
            rec.json_payload,
        ])?;
        Ok(())
    }
}