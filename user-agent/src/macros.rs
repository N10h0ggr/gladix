
/// Logs a structured line with timestamp, level, component, pid, tid, and message.
/// Usage:
/// ```rust
/// gladix_log!(Level::Info, "service", "Service started");
/// gladix_log!(Level::Error, "config", "Config load failed: {}", err);
/// ```
/// Logs like:
/// [2025-04-25T16:32:10+02:00][DEBUG][service][pid=4568][tid=1824] Your message here
#[macro_export]
macro_rules! gladix_log {
    ($level:expr, $component:expr, $fmt:expr $(, $($arg:tt)+)?) => {
        log::log!(
            $level,
            concat!(
                "[", "{}", "]",          // timestamp
                "[", "{}", "]",          // level via Display
                "[", $component, "]",    // component
                "[pid=", "{}", "]",      // pid
                "[tid=", "{:?}", "] ",   // tid
                $fmt                     // your message
            ),
            chrono::Local::now().to_rfc3339(),
            $level,
            std::process::id(),
            std::thread::current().id()
            $(, $($arg)+)?
        );
    };
}



mod tests {
    use super::*;
    use log::{Level, LevelFilter, Log, Metadata, Record};
    use std::sync::Mutex;

    /// A tiny in-memory logger that captures up to DEBUG.
    struct MemoryLogger {
        buffer: Mutex<String>,
    }

    impl MemoryLogger {
        const fn new() -> Self {
            MemoryLogger { buffer: Mutex::new(String::new()) }
        }

        fn take(&self) -> String {
            std::mem::take(&mut *self.buffer.lock().unwrap())
        }
    }

    static LOGGER: MemoryLogger = MemoryLogger::new();

    impl Log for MemoryLogger {
        fn enabled(&self, metadata: &Metadata) -> bool {
            metadata.level() <= Level::Debug
        }
        fn log(&self, record: &Record) {
            if self.enabled(record.metadata()) {
                let mut buf = self.buffer.lock().unwrap();
                buf.push_str(&format!("{}\n", record.args()));
            }
        }
        fn flush(&self) {}
    }

    #[test]
    fn gladix_log_emits_expected_text() {
        // install our in-memory logger
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(LevelFilter::Debug);

        // clear any existing
        LOGGER.take();

        // use the macro
        gladix_log!(Level::Debug, "file:function", "Answer={}!", 42);

        let output = LOGGER.take();
        assert!(output.contains("[DEBUG][file:function]"),   "missing level/component: {}", output);
        assert!(output.contains("Answer=42!"),           "missing payload: {}", output);
        assert!(output.starts_with('['),                 "should start with timestamp: {}", output);
    }
}

