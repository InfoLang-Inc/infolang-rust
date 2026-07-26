use std::env;

/// Supplies Authorization headers and credential metadata.
pub trait AuthProvider: Send + Sync {
    fn apply(&self, headers: &mut reqwest::header::HeaderMap);
    fn pinned_namespace(&self) -> Option<&str> {
        None
    }
}

impl AuthProvider for ApiKeyAuth {
    fn apply(&self, headers: &mut reqwest::header::HeaderMap) {
        use reqwest::header::{HeaderValue, AUTHORIZATION};
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", self.key)) {
            headers.insert(AUTHORIZATION, value);
        }
    }
}

#[allow(deprecated)]
impl AuthProvider for DevKeyAuth {
    fn apply(&self, headers: &mut reqwest::header::HeaderMap) {
        use reqwest::header::{HeaderValue, AUTHORIZATION};
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", self.key)) {
            headers.insert(AUTHORIZATION, value);
        }
    }

    fn pinned_namespace(&self) -> Option<&str> {
        Some(&self.namespace)
    }
}

/// Bearer authentication with a managed-cloud API key (`il_live_...`).
#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    pub key: String,
}

impl ApiKeyAuth {
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }
}

/// Self-hosted dev key in `key:namespace` form.
#[deprecated(
    since = "0.3.0",
    note = "the direct endpoint is gone; use an API key against the gateway"
)]
#[derive(Debug, Clone)]
pub struct DevKeyAuth {
    pub key: String,
    pub namespace: String,
}

#[allow(deprecated)]
impl DevKeyAuth {
    pub fn parse(raw: &str) -> Result<Self, crate::ConfigError> {
        let Some((key, namespace)) = raw.split_once(':') else {
            return Err(crate::ConfigError {
                message: "dev key must be in 'key:namespace' form".into(),
            });
        };
        if key.is_empty() {
            return Err(crate::ConfigError {
                message: "dev key must be in 'key:namespace' form".into(),
            });
        }
        Ok(Self {
            key: key.to_string(),
            namespace: namespace.to_string(),
        })
    }
}

#[allow(deprecated)]
pub(crate) fn resolve_auth(
    api_key: Option<String>,
    dev_key: Option<String>,
) -> Result<Box<dyn AuthProvider>, crate::ConfigError> {
    if let Some(key) = api_key {
        return Ok(Box::new(ApiKeyAuth::new(key)));
    }
    if let Some(raw) = dev_key {
        return Ok(Box::new(DevKeyAuth::parse(&raw)?));
    }
    if let Ok(key) = env::var("INFOLANG_API_KEY") {
        if !key.is_empty() {
            return Ok(Box::new(ApiKeyAuth::new(key)));
        }
    }
    if let Ok(raw) = env::var("INFOLANG_DEV_KEY") {
        if !raw.is_empty() {
            return Ok(Box::new(DevKeyAuth::parse(&raw)?));
        }
    }
    Err(crate::ConfigError {
        message: "no credentials: pass an api key, dev_key, or set INFOLANG_API_KEY".into(),
    })
}

/// The base URL always defaults to the managed gateway; the old dev-key
/// redirect to the direct endpoint is gone.
pub(crate) fn resolve_base_url(base_url: Option<String>) -> String {
    if let Some(url) = base_url {
        return url;
    }
    if let Ok(url) = env::var("INFOLANG_BASE_URL") {
        if !url.is_empty() {
            return url;
        }
    }
    crate::CLOUD_BASE_URL.to_string()
}

pub(crate) fn resolve_namespace(namespace: Option<String>, auth: &dyn AuthProvider) -> String {
    if let Some(ns) = namespace {
        return ns;
    }
    if let Some(ns) = auth.pinned_namespace() {
        return ns.to_string();
    }
    env::var("INFOLANG_NAMESPACE").unwrap_or_default()
}

pub(crate) fn resolve_workspace(workspace: Option<String>) -> Option<String> {
    workspace
        .or_else(|| {
            env::var("INFOLANG_WORKSPACE")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            env::var("INFOLANG_WORKSPACE_ID")
                .ok()
                .filter(|s| !s.is_empty())
        })
}
