# Gladix

**Gladix** is a lightweight, modular EDR (Endpoint Detection and Response) system built in Rust for Windows environments.  
It is designed for learning, experimentation, and research, featuring a user-mode agent and a kernel-mode driver that work together to monitor file, process, and network activity.


## 🧩 Architecture Overview

Gladix consists of two main components:


### 🖥️ [`user-agent`](./user-agent/)

A Windows service responsible for static file analysis, telemetry processing, and detection logic.  
It communicates with both the kernel and a local GUI.

- Scans directories using YARA + heuristics  
- Persistent caching with hash + mtime  
- Receives kernel telemetry (via FilterSendMessage, IOCTL)  
- Writes to SQLite (WAL) in batches  
- Provides a gRPC interface to the UI  


### 🧠 [`kernel-driver`](./kernel-driver/)

A Rust-based Windows kernel-mode driver that hooks into key system events.

- Filesystem monitoring via minifilter  
- Network flow inspection via WFP callouts  
- Process creation via PsNotify + callbacks  
- Sends data to user-agent, receives policies  


## 🗺 Architecture Diagram

Below is a visual representation of how the system components interact:

![Gladix Architecture Diagram](./arquitectura.png)


## 📁 Project Structure

```text
Gladix/
├── user-agent/        # User-mode agent service (file scanning, rule engine)
├── kernel-driver/     # Kernel-mode driver (minifilter, WFP, callbacks)
├── Cargo.toml         # Workspace definition
└── .cargo/config.toml # Optional linker/target overrides
```


## 🛠 Build Instructions

### Requirements

- 🦀 Rust (latest stable + nightly)  
- 🪟 Windows (x86_64, test-signing enabled)  
- 📦 Visual Studio Build Tools / WDK (for driver)  
- 💾 SQLite (bundled via Rust crate)  


### Build All

```bash
cargo build --release
```

### Run the Agent

```bash
cd user-agent
cargo run --release
```

### Install the Driver

> ⚠️ Requires `bcdedit /set testsigning on` and a reboot

```cmd
sc create gladix_driver type= kernel binPath= "C:\path\to\kernel-driver.sys"
sc start gladix_driver
```


## 🧪 Project Goals

Gladix is meant to help you learn about:

- How kernel ↔ user communication works on Windows  
- Writing and testing minifilters and WFP callouts  
- Structuring an EDR pipeline from scratch  
- Static analysis of binaries with YARA  
- Avoiding common performance and stability pitfalls in security software  
```

