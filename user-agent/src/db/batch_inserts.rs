use crate::comms::events::{FileEvent, NetworkEvent, EtwEvent};
use crate::db::db_writer::BatchInsert;

/// FILE EVENTS
impl BatchInsert<FileEvent> for FileEvent {
    fn insert_sql() -> &'static str {
        "INSERT INTO fs_events
           (ts, sensor_guid, op, path, new_path, pid, exe_path, size, sha256, result)
         VALUES
           (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
    }

    fn bind_params(stmt: &mut rusqlite::Statement<'_>, ev: &FileEvent) -> rusqlite::Result<()> {
        stmt.raw_bind_parameter(1, ev.ts.timestamp_micros())?;
        stmt.raw_bind_parameter(2, &ev.sensor_guid)?;
        stmt.raw_bind_parameter(3, &format!("{:?}", ev.op))?;
        stmt.raw_bind_parameter(4, &ev.path)?;
        stmt.raw_bind_parameter(5, &ev.new_path.clone().unwrap_or_default())?;
        stmt.raw_bind_parameter(6, ev.pid as i64)?;
        stmt.raw_bind_parameter(7, &ev.exe_path)?;
        stmt.raw_bind_parameter(8, ev.size as i64)?;
        stmt.raw_bind_parameter(9, &ev.sha256)?;
        stmt.raw_bind_parameter(10, ev.success.to_string())?;
        Ok(())
    }
}

/// NETWORK EVENTS
impl BatchInsert<NetworkEvent> for NetworkEvent {
    fn insert_sql() -> &'static str {
        "INSERT INTO network_events
           (ts, sensor_guid, direction, proto, src_ip, src_port,
            dst_ip, dst_port, pid, exe_path, bytes, verdict)
         VALUES
           (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
    }

    fn bind_params(stmt: &mut rusqlite::Statement<'_>, ev: &NetworkEvent) -> rusqlite::Result<()> {
        stmt.raw_bind_parameter(1, ev.ts.timestamp_micros())?;
        stmt.raw_bind_parameter(2, &ev.sensor_guid)?;
        stmt.raw_bind_parameter(3, &format!("{:?}", ev.direction))?;
        stmt.raw_bind_parameter(4, &ev.proto)?;
        stmt.raw_bind_parameter(5, &ev.src_ip)?;
        stmt.raw_bind_parameter(6, ev.src_port as i64)?;
        stmt.raw_bind_parameter(7, &ev.dst_ip)?;
        stmt.raw_bind_parameter(8, ev.dst_port as i64)?;
        stmt.raw_bind_parameter(9, ev.pid as i64)?;
        stmt.raw_bind_parameter(10, &ev.exe_path)?;
        stmt.raw_bind_parameter(11, ev.bytes as i64)?;
        stmt.raw_bind_parameter(12, ev.blocked.to_string())?;
        Ok(())
    }
}

/// ETW EVENTS
impl BatchInsert<EtwEvent> for EtwEvent {
    fn insert_sql() -> &'static str {
        "INSERT INTO etw_events
           (ts, sensor_guid, provider_guid, event_id, level, pid, tid, json_payload)
         VALUES
           (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    }

    fn bind_params(stmt: &mut rusqlite::Statement<'_>, ev: &EtwEvent) -> rusqlite::Result<()> {
        stmt.raw_bind_parameter(1, ev.ts.timestamp_micros())?;
        stmt.raw_bind_parameter(2, &ev.sensor_guid)?;
        stmt.raw_bind_parameter(3, &ev.provider_guid)?;
        stmt.raw_bind_parameter(4, ev.event_id as i64)?;
        stmt.raw_bind_parameter(5, ev.level as i64)?;
        stmt.raw_bind_parameter(6, ev.pid as i64)?;
        stmt.raw_bind_parameter(7, ev.tid as i64)?;
        stmt.raw_bind_parameter(8, &ev.json_payload)?;
        Ok(())
    }
}
