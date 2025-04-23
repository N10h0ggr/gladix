//! Pre-create operation filter for file access events.
//!
//! This module defines the logic for filtering and inspecting file open
//! operations (`IRP_MJ_CREATE`). It evaluates criteria such as extension,
//! path, or flags, and decides whether to forward the event to user space
//! for further analysis.
//!
//! Key responsibilities:
//! - Intercept `IRP_MJ_CREATE` in pre-operation callbacks.
//! - Apply filtering rules to reduce overhead (e.g., skip harmless files).
//! - Prepare and format messages for user-agent delivery.
//! - Return early (`FLT_PREOP_SUCCESS_NO_CALLBACK`) for ignored cases.
