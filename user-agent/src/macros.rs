/// A flexible logger that prints different fields by level:
/// Usage:
/// ```rust
/// gladix_log!(Level::Info, "service", "Service started");
/// gladix_log!(Level::Error, "config", "Config load failed: {}", err);
/// ```
/// INFO  → [ts][INFO][file:function] msg
/// WARN  → [ts][WARN][file:function] [pid=…] msg
/// DEBUG → [ts][DEBUG][file:function] [pid=…][tid=…] msg
/// ERROR → [ts][ERROR][file:function:line] [pid=…][tid=…] msg
#[macro_export]
macro_rules! gladix_log {
    // INFO: only timestamp, level, file:function
    (Level::Info, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::info!(
            concat!(
                "[{}][INFO][{}:{}] ", // ts + literal level + file:function
                $fmt
            ),
            chrono::Local::now().format("%+"),
            file!(), module_path!()
            $(, $($arg)+)?
        );
    };

    // WARN: add pid
    (Level::Warn, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::warn!(
            concat!(
                "[{}][WARN][{}:{}] [pid={}] ", // + pid
                $fmt
            ),
            chrono::Local::now().format("%+"),
            file!(), module_path!(),
            std::process::id()
            $(, $($arg)+)?
        );
    };

    // DEBUG: add pid + tid
    (Level::Debug, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::debug!(
            concat!(
                "[{}][DEBUG][{}:{}] [pid={}][tid={:?}] ", // + pid + tid
                $fmt
            ),
            chrono::Local::now().format("%+"),
            file!(), module_path!(),
            std::process::id(), std::thread::current().id()
            $(, $($arg)+)?
        );
    };

    // ERROR: add file:function:line + pid + tid
    (Level::Error, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::error!(
            concat!(
                "[{}][ERROR][{}:{}:{}] [pid={}][tid={:?}] ", // file:function:line + pid + tid
                $fmt
            ),
            chrono::Local::now().format("%+"),
            file!(), module_path!(), line!(),
            std::process::id(), std::thread::current().id()
            $(, $($arg)+)?
        );
    };

    // Fallback for any other level
    ($level:expr, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::log!(
            $level,
            concat!(
                "[{}][{}][{}:{}] [pid={}][tid={:?}] ",
                $fmt
            ),
            chrono::Local::now().format("%+"),
            $level,
            file!(), module_path!(),
            std::process::id(), std::thread::current().id()
            $(, $($arg)+)?
        );
    };
}


