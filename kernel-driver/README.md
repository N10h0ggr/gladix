# EDR Kernel Driver

This crate contains the **kernel-mode driver** for a lightweight, educational EDR (Endpoint Detection and Response) system.  
It is designed for Windows environments and written in `#![no_std]` Rust with bindings to the Windows Kernel API. The driver provides low-level visibility and telemetry from the file system, process management, and network stack.

> 🧪 This driver is meant for development, learning, and testing — **not** for production use.

---

## 🧩 Responsibilities

The driver is responsible for:

- **File system monitoring**: Using a minifilter to intercept create/write operations.
- **Network flow inspection**: Using WFP callouts to track outgoing connections.
- **Process telemetry**: Registering callbacks on process creation and object access.
- **Kernel ↔ user communication**:
  - Send telemetry to user-agent via `FltSendMessage`.
  - Receive policy/config updates via IOCTL or ring buffer.
- **Inline hooking** *(optional)*: Placeholder for research on userland API monitoring.

---

## 📁 Project structure

```text
src/
├── lib.rs                // DriverEntry and integration of all components
├── core.rs               // Global IOCTL handling and driver lifecycle logic
├── minifilter/           // File I/O inspection logic
│   ├── mod.rs
│   ├── precreate.rs      // Filter IRP_MJ_CREATE and early event selection
│   └── sendmsg.rs        // Send file telemetry to user-agent
├── wfp/                  // Network flow monitoring
│   ├── mod.rs
│   └── ale_flow.rs       // Hook into ALE_FLOW_ESTABLISHED for new flows
├── callbacks/            // Process and object callback registration
│   ├── mod.rs
│   └── psnotify.rs       // Track process creation and PID relationships
├── hooks.rs              // (Optional) Inline hooking logic for userland APIs
└── tests/                // Mock tests simulating kernel logic in user-mode
```

---

## ⚙️ Build Instructions

This crate is designed to compile to a Windows `.sys` driver binary.

### Prerequisites

- Rust nightly
- Windows target toolchain: `x86_64-pc-windows-msvc`
- Kernel-mode Rust crate setup (e.g., [`windows-kernel-rs`](https://github.com/microsoft/windows-rs))
- Proper build environment (Visual Studio Build Tools or WDK)

### Build

```bash
cargo build --release -p kernel-driver
```

This will produce a `.sys` file in `target/x86_64-pc-windows-msvc/release/`.

### Install (manual testing only)

Enable test mode:

```bash
bcdedit /set testsigning on
shutdown /r /t 0
```

Install driver (manually or via .INF):

```cmd
sc create edr_driver type= kernel binPath= "C:\path\to\kernel-driver.sys"
sc start edr_driver
```

---

## 🛤 Planned Features

- [ ] Automatic passthrough mode on failure
- [ ] WFP stream layer analysis (deeper packet-level inspection)
- [ ] ETW provider integration from kernel space
- [ ] In-memory filtering cache to reduce driver-to-user traffic
- [ ] Binary signing and PPL support (for learning)

---

Made with Rust, ring buffers, and the smell of BSODs. 💥
