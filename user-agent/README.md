# EDR User-Agent

This crate implements the **user-mode service** for a custom lightweight EDR (Endpoint Detection and Response) system.  
It is designed to run as a Windows service and acts as the central controller for user-space telemetry, static analysis, rule evaluation, and communication with the kernel driver and GUI.

---

## 🧩 Responsibilities

The user-agent is responsible for:

- **Detection logic**: Applies rule-based or heuristic logic to telemetry.
- **Static file scanning**: Recursively scans configured directories using YARA + filters.
- **Persistent cache**: Avoids redundant scans via timestamp and hash-based caching.
- **Communication**:
  - With the **kernel driver** (via IOCTL, FilterSendMessage).
  - With the **GUI** (via local gRPC server).
- **ETW event consumption**: Subscribes to system-level telemetry like process creation and image loads.
- **Scheduled task execution**: Periodically scans risk-classified directories.

---

## 📁 Project structure

```text
src/
├── main.rs              // Windows service entry point
├── scanner/             // Static file scanning logic
│   ├── cache.rs         // Persistent scan result cache with HMAC validation
│   ├── hash.rs          // File hashing and pre-filtering (size, extension)
│   ├── worker.rs        // File processing threads and cache updates
│   └── scheduler.rs     // Recursion + scheduled scanning
├── config/              // Runtime and TOML configuration types
│   ├── types.rs         // Risk groups, intervals, limits, etc.
│   └── loader.rs        // Loads and converts config from file
├── db/                  // (Planned) SQLite WAL database integration
├── comms/               // (Planned) IPC between kernel, GUI and agent
├── intel/               // (Planned) Detection engine and rule pipeline
├── etw/                 // (Planned) ETW provider consumption
└── tests/               // Integration and feature-specific tests
```

---

## 🛠 Build & Run

> ❗ This binary is intended to run on Windows systems.

### Build

```bash
cargo build --release -p user-agent
```

### Run (for now, runs in foreground with logs)

```bash
./target/release/user-agent
```

### Config file

Place a configuration file named `agent_config.toml` in the working directory.  
It should define scan intervals, directory groups, and optional limits.

---

## 💡 Future Scope

This crate is designed to be modular and extensible.  
Planned enhancements include:

- [ ] SQLite-based event storage
- [ ] Real-time communication with GUI (gRPC)
- [ ] Policy updates from remote control plane
- [ ] Alert response actions (kill process, quarantine file, etc.)
- [ ] Full Windows service registration (`sc.exe` / registry)
- [ ] Multi-user support and GUI RBAC


---

Made with 🦀 and curiosity.
