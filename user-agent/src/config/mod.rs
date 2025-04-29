//! Public API for configuration

pub mod loader;
pub mod model;

// Re-export the main entrypoints:
pub use loader::load;
pub use model::Config;
