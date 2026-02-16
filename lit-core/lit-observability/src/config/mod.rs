use crate::error::Result;
use lit_core::config::{LitConfig, LitConfigBuilder};

pub const CFG_KEY_LOGGING_LEVEL: &str = "logging.level";

pub const DEFAULT_LOGGING_LEVEL: &str = "trace";

pub trait LitObservabilityConfig {
    fn apply_defaults(builder: LitConfigBuilder) -> Result<LitConfigBuilder>;
    fn logging_level(&self) -> Result<String>;
}

impl LitObservabilityConfig for LitConfig {
    #[inline]
    fn apply_defaults(mut builder: LitConfigBuilder) -> Result<LitConfigBuilder> {
        // Set defaults
        builder = builder.set_default(CFG_KEY_LOGGING_LEVEL, DEFAULT_LOGGING_LEVEL);

        Ok(builder)
    }

    #[inline]
    fn logging_level(&self) -> Result<String> {
        self.get_string(CFG_KEY_LOGGING_LEVEL)
    }
}
