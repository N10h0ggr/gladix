//! Process creation notification handler.
//!
//! This module focuses on intercepting process creation events to extract
//! metadata (PID, parent PID, image path, etc.) for correlation and
//! monitoring purposes. It enables early detection of suspicious process trees.
//!
//! Key responsibilities:
//! - Handle `PsCreateProcessNotifyEx` events.
//! - Build event structures with parent/child info.
//! - Track relationships between processes for runtime intelligence.
//! - Pass data to user space via configured channels.
