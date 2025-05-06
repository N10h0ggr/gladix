#![cfg_attr(feature = "kernel", no_std)]

#[cfg(feature = "kernel")]
extern crate alloc; // gives Vec and String

pub mod events {
    include!("proto_gen/events.rs"); // or mod per file
}

pub mod constants;
