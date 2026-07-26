//! Official async Rust client for [InfoLang](https://infolang.ai) semantic memory.
//!
//! Targets the InfoLang gateway `/v2` API (<https://api.infolang.ai/docs>) with
//! typed requests, automatic retries, and workspace path scoping
//! (`/v2/workspaces/{workspace_id}/...`). Workspace resolution is automatic: a
//! credential with exactly one workspace grant needs no configuration; pass
//! `.workspace(...)` (or set `INFOLANG_WORKSPACE`) otherwise.
//!
//! # Quickstart
//!
//! ```no_run
//! use infolang::Client;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), infolang::Error> {
//!     let client = Client::new("il_live_...")?;
//!     let result = client
//!         .investigate("how does auth middleware work?", None)
//!         .await?;
//!     for chunk in result.chunks {
//!         println!("{:.3}  {}", chunk.score.unwrap_or(0.0), chunk.text);
//!     }
//!     Ok(())
//! }
//! ```

mod auth;
mod client;
mod error;
mod health;
mod memory;
mod ops;
mod transport;
mod types;

pub use auth::ApiKeyAuth;
#[allow(deprecated)]
pub use auth::DevKeyAuth;
pub use client::{Client, ClientBuilder};
pub use error::{ApiError, ConfigError, ConnectionError, Error};
pub use types::*;

/// Managed cloud gateway base URL — always the default.
pub const CLOUD_BASE_URL: &str = "https://api.infolang.ai";

/// Self-hosted base URL.
#[deprecated(
    since = "0.3.0",
    note = "the direct endpoint is gone; kept only so 0.2.x code compiles"
)]
pub const DIRECT_BASE_URL: &str = "http://127.0.0.1:8766";

/// Crate semver; stamped into the default User-Agent header.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
