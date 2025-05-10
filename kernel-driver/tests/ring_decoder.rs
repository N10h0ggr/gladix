use shared::events::base_event::Payload;
use shared::events::BaseEvent;
use prost::Message;

/// Helper to compute how many bytes a u64 varint takes.
fn varint_len(mut v: u64) -> usize {
    let mut len = 1;
    while v >= 0x80 {
        v >>= 7;
        len += 1;
    }
    len
}

fn decode_events(raw: &[u8]) -> Vec<BaseEvent> {
    let mut events = Vec::new();
    let mut cursor = 4; // skip ring’s write_offset

    while cursor + 4 <= raw.len() {
        // read length prefix
        let msg_len = u32::from_le_bytes(raw[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;

        if cursor + msg_len > raw.len() {
            eprintln!(
                "Incomplete message at {}: need {} bytes, only {} available",
                cursor,
                msg_len,
                raw.len() - cursor
            );
            break;
        }

        // slice out exactly the bytes for this one BaseEvent
        let slice = &raw[cursor..cursor + msg_len];
        match BaseEvent::decode(slice) {
            Ok(evt) => {
                // compute the on‐the‐wire bytes for each field:

                // 1) ts field (field#1, wire type=length-delim)
                let ts_bytes = if let Some(ts) = &evt.ts {
                    let inner = ts.encoded_len();
                    1 + varint_len(inner as u64) + inner
                } else {
                    0
                };

                // 2) sensor_guid field (field#2, wire type=length-delim)
                let guid = &evt.sensor_guid;
                let guid_len = guid.len();
                let guid_bytes = 1 + varint_len(guid_len as u64) + guid_len;

                // 3) payload field (field#3, wire type=length-delim)
                let (payload_bytes, payload_debug) = if let Some(Payload::ProcessEvent(pe)) = &evt.payload {
                    let inner = pe.encoded_len();
                    let bytes = 1 + varint_len(inner as u64) + inner;
                    (bytes, format!("{:?}", pe))
                } else {
                    (0, format!("{:?}", evt.payload))
                };

                // print in Debug‐like style + annotations
                println!("--- Event #{} ---", events.len() + 1);
                println!("BaseEvent {{");
                println!("    ts: {:?}    // {} bytes", evt.ts, ts_bytes);
                println!("    sensor_guid: {:?}    // {} bytes", guid, guid_bytes);
                println!(
                    "    payload: Some(\n        ProcessEvent({})\n    )    // {} bytes",
                    payload_debug, payload_bytes
                );
                println!("}}\n");

                events.push(evt);
            }
            Err(e) => {
                eprintln!("Decode error at offset {}: {}", cursor, e);
                break;
            }
        }

        cursor += msg_len;
    }

    events
}

fn main() {
    const RAW_DATA: &[u8] = &[ /*...*/];

    let evs = decode_events(RAW_DATA);
    println!("Decoded {} event(s)", evs.len());
}
