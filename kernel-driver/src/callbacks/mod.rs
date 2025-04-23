//! System callbacks for process and object activity.
//!
//! This module registers and handles low-level system callbacks provided by
//! the Windows kernel to observe sensitive events like process creation and
//! access to protected objects.
//!
//! Key responsibilities:
//! - Use `PsSetCreateProcessNotifyRoutine` for process tracking.
//! - Register object access callbacks using `ObRegisterCallbacks`.
//! - Forward relevant events to the user-agent for policy enforcement.
//! - Deregister callbacks on driver unload.
