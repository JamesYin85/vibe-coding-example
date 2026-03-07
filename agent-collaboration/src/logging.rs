use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub env_filter: String,
    pub with_ansi: bool,
    pub with_target: bool,
    pub with_thread_ids: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            env_filter: "info,agent_collaboration=debug".to_string(),
            with_ansi: true,
            with_target: true,
            with_thread_ids: false,
        }
    }
}

impl LoggingConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_filter(mut self, filter: impl Into<String>) -> Self {
        self.env_filter = filter.into();
        self
    }

    pub fn without_ansi(mut self) -> Self {
        self.with_ansi = false;
        self
    }

    pub fn without_target(mut self) -> Self {
        self.with_target = false;
        self
    }
}

pub fn init_logging() {
    init_logging_with_config(LoggingConfig::default());
}

pub fn init_logging_with_config(config: LoggingConfig) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.env_filter));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(config.with_ansi)
                .with_target(config.with_target)
                .with_thread_ids(config.with_thread_ids),
        )
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LoggingConfig::default();
        assert!(config.with_ansi);
        assert!(config.with_target);
    }

    #[test]
    fn test_config_builder() {
        let config = LoggingConfig::new()
            .with_filter("debug")
            .without_ansi()
            .without_target();
        assert!(!config.with_ansi);
        assert!(!config.with_target);
    }
}
