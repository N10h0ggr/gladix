// src/macros.rs

/// Per‐level logging macro.  Always includes PID and a numeric TID (via `as_u64()`),
/// and only emits _one_ `[LEVEL]` prefix of your choosing:
///
/// INFO  → [ts][INFO][file:function] [pid=…][tid=…] msg  
/// WARN  → [ts][WARN][file:function] [pid=…][tid=…] msg  
/// DEBUG → [ts][DEBUG][file:function] [pid=…][tid=…] msg  
/// ERROR → [ts][ERROR][file:function:line] [pid=…][tid=…] msg  
#[macro_export]
macro_rules! gladix_log {
    // INFO
    (Level::Info, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::info!(
            concat!("[{}][INFO][pid={}][tid={:?}][{}] ", $fmt),
            chrono::Local::now().format("%+"),
            std::process::id(), std::thread::current().id(),
            module_path!()
            $(, $($arg)+)?
        );
    };
    // WARN
    (Level::Warn, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::warn!(
            concat!("[{}][WARN][pid={}][tid={:?}][{}] ", $fmt),
            chrono::Local::now().format("%+"),
            std::process::id(), std::thread::current().id(),
            module_path!()
            $(, $($arg)+)?
        );
    };
    // DEBUG
    (Level::Debug, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::debug!(
            concat!("[{}][DEBUG][pid={}][tid={:?}][{}] ", $fmt),
            chrono::Local::now().format("%+"),
            std::process::id(), std::thread::current().id(),
            module_path!()
            $(, $($arg)+)?
        );
    };
    // ERROR
    (Level::Error, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::error!(
            concat!("[{}][ERROR][pid={}][tid={:?}][{}] ", $fmt),
            chrono::Local::now().format("%+"),
            std::process::id(), std::thread::current().id(),
            module_path!()
            $(, $($arg)+)?
        );
    };
    // fallback for other levels
    ($lvl:expr, $fmt:expr $(, $($arg:tt)+)? ) => {
        log::log!(
            $lvl,
            concat!("[{}][{:?}][pid={}][tid={:?}][{}] ", $fmt),
            chrono::Local::now().format("%+"),
            $lvl,
            std::process::id(), std::thread::current().id(),
            module_path!()
            $(, $($arg)+)?
        );
    };
}
