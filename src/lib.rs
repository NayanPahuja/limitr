pub mod config;
pub mod errors;
pub mod limiter;
pub mod metrics;
pub mod redis;
pub mod server;
pub mod generated {
    // absolute path relative to crate root:
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/gen/limitr.v1.rs"));
}

// Re-export commonly used types
pub use config::{AppConfig, ConfigCache};
pub use errors::{RateLimitError, Result};
pub use server::{ServerConfig, start_server};
