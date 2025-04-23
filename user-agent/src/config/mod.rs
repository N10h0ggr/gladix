//! API pública de configuración.

pub mod types;
pub mod loader;

pub use types::{DirectoryRisk, RiskGroup};
pub use loader::{load_master_config, convert_config_to_risk_group};
