//! bollard connection helpers.

use crate::config;
use crate::error::{AppError, AppResult};
use bollard::Docker;

/// Connect a bollard client to the LiteDock engine over local TCP.
/// A 120s timeout accommodates long-running calls (pull, build via API, …).
pub fn connect() -> AppResult<Docker> {
    let url = config::engine_http_url();
    Docker::connect_with_http(&url, 120, bollard::API_DEFAULT_VERSION).map_err(AppError::from)
}
