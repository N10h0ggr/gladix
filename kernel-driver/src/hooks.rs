//! Optional inline hooking for user-mode API monitoring.
//!
//! This module provides utilities to install inline hooks into selected
//! user-mode APIs from within the kernel. These hooks are used to detect
//! techniques like process injection, API abuse, or behavior evasion.
//!
//! ⚠️ **Warning:** This module is highly sensitive to system stability
//! and should be used with caution. It is disabled by default and intended
//! for research, prototyping, or future extensions.
//!
//! Key responsibilities:
//! - Define trampolines and detour functions.
//! - Locate target functions in user-space modules.
//! - Patch function prologues safely (if enabled).
//! - Provide interface to enable/disable hooks from user agent.
//!
//! This functionality is architecture- and version-specific, and should
//! not be included in production builds without rigorous testing.
