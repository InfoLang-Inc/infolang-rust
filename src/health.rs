use reqwest::Method;
use serde_json::Value;

use crate::client::Client;
use crate::Error;

impl Client {
    /// Liveness (`GET /healthz`). Non-object payloads are wrapped as
    /// `{"status": <payload>}`.
    pub async fn health(&self) -> Result<Value, Error> {
        let resp = self
            .transport()
            .do_json(Method::GET, "/healthz", None)
            .await?;
        Ok(wrap_status(resp.data))
    }

    /// Readiness (`GET /readyz`) — the authoritative "is the engine up"
    /// signal.
    pub async fn ready(&self) -> Result<Value, Error> {
        let resp = self
            .transport()
            .do_json(Method::GET, "/readyz", None)
            .await?;
        Ok(wrap_status(resp.data))
    }
}

fn wrap_status(data: Value) -> Value {
    if data.is_object() {
        data
    } else {
        serde_json::json!({ "status": data })
    }
}
