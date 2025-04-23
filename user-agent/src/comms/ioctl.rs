//! IOCTL communication interface with the kernel driver.
//!
//! This module wraps low-level Windows DeviceIoControl operations,
//! providing a safe Rust interface to send configuration commands
//! and retrieve data from the kernel-mode EDR driver.
//!
//! Key responsibilities:
//! - Open a secure handle to the driver device.
//! - Send control codes (IOCTLs) and marshal data safely.
//! - Handle error codes and fallback conditions.
//! - Abstract over driver protocol versioning (if added).
