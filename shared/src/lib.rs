pub mod events {
    include!("proto_gen/events.rs"); // or mod per file
}

pub mod config {
    include!("proto_gen/config.rs"); // or mod per file
}

pub mod constants;