use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client as HttpClient, Method};
use tokio::sync::OnceCell;

use crate::auth::{resolve_auth, resolve_base_url, resolve_namespace, resolve_workspace};
use crate::error::ConfigError;
use crate::transport::{build_http_client, default_user_agent, Transport};
use crate::types::Whoami;
use crate::Error;

/// Typed async client over the InfoLang gateway `/v2` API.
#[derive(Clone)]
pub struct Client {
    pub namespace: String,
    /// The explicitly configured workspace (builder option or
    /// `INFOLANG_WORKSPACE` / `INFOLANG_WORKSPACE_ID`), when any.
    pub workspace: Option<String>,
    pub base_url: String,
    transport: Arc<Transport>,
    /// Cached whoami-resolved workspace id; failed resolutions are not cached.
    workspace_cache: Arc<OnceCell<String>>,
}

/// Builder for [`Client`].
#[derive(Debug, Default)]
pub struct ClientBuilder {
    api_key: Option<String>,
    dev_key: Option<String>,
    base_url: Option<String>,
    namespace: Option<String>,
    workspace: Option<String>,
    user_agent: Option<String>,
    max_retries: Option<usize>,
    timeout: Option<Duration>,
    http_client: Option<HttpClient>,
}

impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        ClientBuilder::default().api_key(api_key.into()).build()
    }

    pub(crate) fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Identity echo for the current credential (`GET /v2/whoami`).
    pub async fn whoami(&self) -> Result<Whoami, Error> {
        let resp = self
            .transport()
            .do_json(Method::GET, "/v2/whoami", None)
            .await?;
        serde_json::from_value(resp.data).map_err(|err| {
            Error::Config(ConfigError {
                message: format!("failed to decode whoami response: {err}"),
            })
        })
    }

    /// The workspace id every call operates in. Explicit configuration wins
    /// (no whoami call); otherwise resolved once via `whoami` and cached —
    /// single-grant credentials only. Multi-workspace credentials must pass
    /// `workspace`. A failed resolution is not cached; a retry re-asks.
    pub async fn workspace_id(&self) -> Result<String, Error> {
        if let Some(ws) = &self.workspace {
            return Ok(ws.clone());
        }
        self.workspace_cache
            .get_or_try_init(|| async {
                let who = self.whoami().await?;
                let ids: Vec<String> = who
                    .workspaces
                    .iter()
                    .map(|w| w.workspace_id.clone())
                    .filter(|id| !id.is_empty())
                    .collect();
                match ids.len() {
                    1 => Ok(ids.into_iter().next().expect("one id")),
                    0 => Err(Error::Config(ConfigError {
                        message: "this credential has no visible workspace grants; mint a key \
                                  from the Console or pass `workspace` explicitly"
                            .into(),
                    })),
                    n => Err(Error::Config(ConfigError {
                        message: format!(
                            "this credential can reach {n} workspaces ({}); pass `workspace` \
                             (or set INFOLANG_WORKSPACE) to pick one",
                            ids.join(", ")
                        ),
                    })),
                }
            })
            .await
            .cloned()
    }

    /// `/v2/workspaces/{ws}` prefix for the resolved workspace; the id is
    /// opaque and percent-encoded.
    pub(crate) async fn ws_prefix(&self) -> Result<String, Error> {
        let ws = self.workspace_id().await?;
        Ok(format!("/v2/workspaces/{}", urlencoding::encode(&ws)))
    }

    pub(crate) fn default_namespace(&self, namespace: Option<String>) -> Option<String> {
        namespace.or_else(|| {
            if self.namespace.is_empty() {
                None
            } else {
                Some(self.namespace.clone())
            }
        })
    }
}

impl ClientBuilder {
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn dev_key(mut self, dev_key: impl Into<String>) -> Self {
        self.dev_key = Some(dev_key.into());
        self
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Workspace to operate in (opaque id, e.g. from the Console or
    /// `whoami`). Optional for credentials holding exactly one workspace
    /// grant. Also reads `INFOLANG_WORKSPACE` / `INFOLANG_WORKSPACE_ID`.
    pub fn workspace(mut self, workspace: impl Into<String>) -> Self {
        self.workspace = Some(workspace.into());
        self
    }

    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    pub fn max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn http_client(mut self, http_client: HttpClient) -> Self {
        self.http_client = Some(http_client);
        self
    }

    pub fn build(self) -> Result<Client, Error> {
        let auth = resolve_auth(self.api_key, self.dev_key)?;
        let base_url = resolve_base_url(self.base_url);
        let namespace = resolve_namespace(self.namespace, auth.as_ref());
        let workspace = resolve_workspace(self.workspace);
        let timeout = self.timeout.unwrap_or(Duration::from_secs(30));
        let http = match self.http_client {
            Some(http) => http,
            None => build_http_client(timeout)?,
        };
        let user_agent = self.user_agent.unwrap_or_else(default_user_agent);
        let transport = Transport {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
            auth,
            user_agent,
            max_retries: self.max_retries.unwrap_or(2),
            backoff_base: Duration::from_millis(500),
            backoff_cap: Duration::from_secs(8),
        };
        Ok(Client {
            namespace,
            workspace,
            base_url: transport.base_url.clone(),
            transport: Arc::new(transport),
            workspace_cache: Arc::new(OnceCell::new()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CLOUD_BASE_URL;
    use std::env;

    fn clear_env() {
        for key in [
            "INFOLANG_API_KEY",
            "INFOLANG_DEV_KEY",
            "INFOLANG_BASE_URL",
            "INFOLANG_NAMESPACE",
            "INFOLANG_WORKSPACE",
            "INFOLANG_WORKSPACE_ID",
        ] {
            env::remove_var(key);
        }
    }

    #[test]
    fn new_from_api_key_uses_cloud() {
        clear_env();
        let client = Client::new("il_live_abc").expect("client");
        assert_eq!(client.base_url, CLOUD_BASE_URL);
        assert!(client.namespace.is_empty());
        assert!(client.workspace.is_none());
    }

    #[test]
    fn dev_key_pins_namespace_but_not_direct_url() {
        clear_env();
        let client = Client::builder()
            .dev_key("secret:mybank")
            .build()
            .expect("client");
        // 0.3.0: the base URL is ALWAYS the gateway; no direct-endpoint redirect.
        assert_eq!(client.base_url, CLOUD_BASE_URL);
        assert_eq!(client.namespace, "mybank");
    }

    #[test]
    fn dev_key_invalid() {
        clear_env();
        let err = Client::builder().dev_key("nocolon").build();
        assert!(matches!(err, Err(Error::Config(_))));
    }

    #[test]
    fn no_credentials() {
        clear_env();
        let err = Client::builder().build();
        assert!(matches!(err, Err(Error::Config(_))));
    }

    #[tokio::test]
    async fn explicit_workspace_wins_without_whoami() {
        clear_env();
        let client = Client::builder()
            .api_key("il_live_abc")
            .workspace("ws-9")
            .build()
            .expect("client");
        assert_eq!(client.workspace_id().await.expect("ws"), "ws-9");
    }
}
