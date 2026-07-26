use thiserror::Error;

/// Top-level error returned by SDK methods.
#[derive(Debug, Error)]
pub enum Error {
    #[error("infolang: {0}")]
    Config(#[from] ConfigError),
    #[error("infolang: connection error: {source}")]
    Connection {
        #[source]
        source: reqwest::Error,
    },
    #[error("{0}")]
    Api(#[from] ApiError),
}

impl Error {
    pub fn is_authentication(&self) -> bool {
        matches!(self, Error::Api(api) if api.is_authentication())
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::Api(api) if api.is_not_found())
    }

    pub fn is_validation(&self) -> bool {
        matches!(self, Error::Api(api) if api.is_validation())
    }

    pub fn is_rate_limit(&self) -> bool {
        matches!(self, Error::Api(api) if api.is_rate_limit())
    }

    pub fn is_server(&self) -> bool {
        matches!(self, Error::Api(api) if api.is_server())
    }
}

/// Client misconfiguration (missing credentials, bad URL, decode failure).
#[derive(Debug, Clone, Error)]
#[error("infolang: {message}")]
pub struct ConfigError {
    pub message: String,
}

/// Transport failure or timeout.
#[derive(Debug, Error)]
#[error("infolang: connection error: {source}")]
pub struct ConnectionError {
    #[source]
    pub source: reqwest::Error,
}

/// Non-2xx gateway response.
///
/// The gateway `/v2` error envelope is `{"error":{"code","message"}}`; when
/// present, `message` is rendered as `"code: message"` and `code` carries the
/// machine-readable code. Flat `error`/`message`/`detail` bodies still work.
#[derive(Debug, Clone, Error)]
#[error("infolang: {message} (status={status})")]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    /// The gateway's machine-readable error code, when present
    /// (e.g. `lane_not_supported`).
    pub code: Option<String>,
    pub body: serde_json::Value,
    pub request_id: String,
    pub retry_after: f64,
}

impl ApiError {
    /// 401/403 — credential missing, invalid, or lacking permission
    /// (`lane_not_supported` is 403).
    pub fn is_authentication(&self) -> bool {
        self.status == 401 || self.status == 403
    }

    pub fn is_not_found(&self) -> bool {
        self.status == 404
    }

    pub fn is_validation(&self) -> bool {
        self.status == 400 || self.status == 422
    }

    pub fn is_rate_limit(&self) -> bool {
        self.status == 429
    }

    pub fn is_server(&self) -> bool {
        self.status >= 500
    }
}

/// The gateway's machine-readable error code (`{error:{code}}`), when present.
pub(crate) fn error_code(body: &serde_json::Value) -> Option<String> {
    body.as_object()
        .and_then(|obj| obj.get("error"))
        .and_then(|nested| nested.as_object())
        .and_then(|err| err.get("code"))
        .and_then(|code| code.as_str())
        .map(str::to_string)
}

pub(crate) fn message_from_body(body: &serde_json::Value) -> Option<String> {
    if let Some(obj) = body.as_object() {
        // Gateway /v2 envelope: { error: { code, message } }.
        if let Some(nested) = obj.get("error").and_then(|v| v.as_object()) {
            let code = nested.get("code").and_then(|v| v.as_str());
            let message = nested.get("message").and_then(|v| v.as_str());
            match (code, message) {
                (Some(code), Some(message)) => return Some(format!("{code}: {message}")),
                (None, Some(message)) => return Some(message.to_string()),
                (Some(code), None) => return Some(code.to_string()),
                (None, None) => {}
            }
        }
        for key in ["error", "message", "detail"] {
            if let Some(value) = obj.get(key).and_then(|v| v.as_str()) {
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    if let Some(text) = body.as_str() {
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    None
}

pub(crate) fn api_error_from_response(
    status: u16,
    body: serde_json::Value,
    request_id: String,
    retry_after: f64,
) -> ApiError {
    let message =
        message_from_body(&body).unwrap_or_else(|| format!("request failed with status {status}"));
    let code = error_code(&body);
    ApiError {
        status,
        message,
        code,
        body,
        request_id,
        retry_after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_from_body_reads_common_keys() {
        assert_eq!(
            message_from_body(&serde_json::json!({ "error": "boom" })),
            Some("boom".into())
        );
        assert_eq!(
            message_from_body(&serde_json::json!("plain")),
            Some("plain".into())
        );
        assert_eq!(
            message_from_body(&serde_json::json!({ "detail": "oops" })),
            Some("oops".into())
        );
    }

    #[test]
    fn message_from_body_reads_v2_envelope() {
        assert_eq!(
            message_from_body(
                &serde_json::json!({ "error": { "code": "not_found", "message": "missing" } })
            ),
            Some("not_found: missing".into())
        );
        assert_eq!(
            message_from_body(&serde_json::json!({ "error": { "message": "just message" } })),
            Some("just message".into())
        );
        assert_eq!(
            message_from_body(&serde_json::json!({ "error": { "code": "just_code" } })),
            Some("just_code".into())
        );
    }

    #[test]
    fn error_code_extracted() {
        let err = api_error_from_response(
            403,
            serde_json::json!({ "error": { "code": "lane_not_supported", "message": "nope" } }),
            String::new(),
            0.0,
        );
        assert_eq!(err.code.as_deref(), Some("lane_not_supported"));
        assert_eq!(err.message, "lane_not_supported: nope");
        assert!(err.is_authentication());
    }

    #[test]
    fn api_error_classifiers() {
        let err = api_error_from_response(401, serde_json::json!({}), String::new(), 0.0);
        assert!(err.is_authentication());
        assert!(
            api_error_from_response(404, serde_json::json!({}), String::new(), 0.0).is_not_found()
        );
        assert!(
            api_error_from_response(422, serde_json::json!({}), String::new(), 0.0).is_validation()
        );
        assert!(
            api_error_from_response(429, serde_json::json!({}), String::new(), 2.0).is_rate_limit()
        );
        assert!(
            api_error_from_response(500, serde_json::json!({}), String::new(), 0.0).is_server()
        );
    }

    #[test]
    fn top_level_error_helpers() {
        let err = Error::Api(api_error_from_response(
            404,
            serde_json::json!({}),
            String::new(),
            0.0,
        ));
        assert!(err.is_not_found());
    }

    #[test]
    fn top_level_error_rate_limit_and_server() {
        let rate = Error::Api(api_error_from_response(
            429,
            serde_json::json!({}),
            String::new(),
            1.0,
        ));
        assert!(rate.is_rate_limit());
        let server = Error::Api(api_error_from_response(
            500,
            serde_json::json!({}),
            String::new(),
            0.0,
        ));
        assert!(server.is_server());
        let validation = Error::Api(api_error_from_response(
            422,
            serde_json::json!({}),
            String::new(),
            0.0,
        ));
        assert!(validation.is_validation());
    }
}
