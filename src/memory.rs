use reqwest::Method;
use serde_json::{json, Map, Value};

use crate::client::Client;
use crate::types::{
    normalize_tags, parse_execute_remember_batch, parse_list, parse_namespaces, parse_recall,
    parse_remember, ForgetOptions, InvestigateOptions, ListOptions, MemoryPage, NamespaceInfo,
    RecallOptions, RecallResult, RememberItem, RememberOptions, RememberResult,
};
use crate::Error;

impl Client {
    /// Semantic recall (`POST /v2/workspaces/{ws}/recall`).
    ///
    /// The retrieval extras (`golden`, `format`, `snippet_chars`, `adaptive`,
    /// `margin`) are serialized only when set. Reserved: accepted today,
    /// activates in a future release.
    pub async fn recall(
        &self,
        query: &str,
        options: Option<RecallOptions>,
    ) -> Result<RecallResult, Error> {
        let options = options.unwrap_or_default();
        let mut body = Map::new();
        body.insert("query".into(), json!(query));
        if let Some(ns) = self.default_namespace(options.namespace) {
            body.insert("namespace".into(), json!(ns));
        }
        if let Some(top_k) = options.top_k {
            body.insert("top_k".into(), json!(top_k));
        }
        if let Some(verbose) = options.verbose {
            body.insert("verbose".into(), json!(verbose));
        }
        if let Some(golden) = options.golden {
            body.insert("golden".into(), json!(golden));
        }
        if let Some(format) = options.format {
            body.insert("format".into(), json!(format));
        }
        if let Some(snippet_chars) = options.snippet_chars {
            body.insert("snippet_chars".into(), json!(snippet_chars));
        }
        if let Some(adaptive) = options.adaptive {
            body.insert("adaptive".into(), json!(adaptive));
        }
        if let Some(margin) = options.margin {
            body.insert("margin".into(), json!(margin));
        }

        let path = format!("{}/recall", self.ws_prefix().await?);
        let resp = self
            .transport()
            .do_json(Method::POST, &path, Some(Value::Object(body)))
            .await?;
        Ok(parse_recall(&resp.data, resp.metering))
    }

    /// Agent-style recall with a sensible default `top_k` of 5.
    pub async fn investigate(
        &self,
        query: &str,
        options: Option<InvestigateOptions>,
    ) -> Result<RecallResult, Error> {
        let options = options.unwrap_or_default();
        let top_k = options.top_k.unwrap_or(5);
        self.recall(
            query,
            Some(RecallOptions {
                namespace: options.namespace_hint,
                top_k: Some(top_k),
                ..Default::default()
            }),
        )
        .await
    }

    /// Store one memory (`POST /v2/workspaces/{ws}/remember`). Tags are
    /// always sent as an array; comma-joined entries are split.
    pub async fn remember(
        &self,
        text: &str,
        options: Option<RememberOptions>,
    ) -> Result<RememberResult, Error> {
        let options = options.unwrap_or_default();
        let mut body = Map::new();
        body.insert("text".into(), json!(text));
        if let Some(ns) = self.default_namespace(options.namespace) {
            body.insert("namespace".into(), json!(ns));
        }
        if let Some(source) = options.source {
            body.insert("source".into(), json!(source));
        }
        if let Some(tags) = options.tags {
            body.insert("tags".into(), json!(normalize_tags(&tags)));
        }

        let path = format!("{}/remember", self.ws_prefix().await?);
        let resp = self
            .transport()
            .do_json(Method::POST, &path, Some(Value::Object(body)))
            .await?;
        Ok(parse_remember(&resp.data))
    }

    /// Alias for `remember` matching the memorize tool naming.
    pub async fn memorize(
        &self,
        content: &str,
        options: Option<RememberOptions>,
    ) -> Result<RememberResult, Error> {
        self.remember(content, options).await
    }

    /// Client-side sugar over `execute`: one real `remember` sub-op per item
    /// (the `remember_batch` pseudo-op is gone). Empty input sends nothing.
    pub async fn remember_batch(
        &self,
        items: &[RememberItem],
        options: Option<RememberOptions>,
    ) -> Result<Vec<RememberResult>, Error> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let options = options.unwrap_or_default();
        let namespace = self.default_namespace(options.namespace);
        let operations: Vec<Value> = items
            .iter()
            .map(|item| {
                let mut args = Map::new();
                args.insert("text".into(), json!(item.text));
                let source = item.source.clone().or_else(|| options.source.clone());
                if let Some(source) = source {
                    args.insert("source".into(), json!(source));
                }
                if let Some(tags) = &item.tags {
                    args.insert("tags".into(), json!(normalize_tags(tags)));
                }
                if let Some(ns) = &namespace {
                    args.insert("namespace".into(), json!(ns));
                }
                json!({ "op": "remember", "args": Value::Object(args) })
            })
            .collect();

        let path = format!("{}/execute", self.ws_prefix().await?);
        let resp = self
            .transport()
            .do_json(
                Method::POST,
                &path,
                Some(json!({ "operations": operations })),
            )
            .await?;
        Ok(parse_execute_remember_batch(&resp.data))
    }

    /// Delete one memory (`DELETE /v2/workspaces/{ws}/memories/{id}`); the
    /// namespace is sent as a query parameter when set.
    pub async fn forget(
        &self,
        memory_id: &str,
        options: Option<ForgetOptions>,
    ) -> Result<(), Error> {
        let options = options.unwrap_or_default();
        let mut path = format!(
            "{}/memories/{}",
            self.ws_prefix().await?,
            urlencoding::encode(memory_id)
        );
        if let Some(ns) = self.default_namespace(options.namespace) {
            path.push_str(&format!("?namespace={}", urlencoding::encode(&ns)));
        }
        self.transport()
            .do_json(Method::DELETE, &path, None)
            .await?;
        Ok(())
    }

    /// List (or, with `query`, semantically search) memories
    /// (`GET /v2/workspaces/{ws}/memories?ns=&q=&limit=`).
    pub async fn list(&self, options: Option<ListOptions>) -> Result<MemoryPage, Error> {
        let options = options.unwrap_or_default();
        let mut params = Vec::new();
        if let Some(ns) = self.default_namespace(options.namespace) {
            params.push(format!("ns={}", urlencoding::encode(&ns)));
        }
        if let Some(query) = options.query {
            params.push(format!("q={}", urlencoding::encode(&query)));
        }
        if let Some(limit) = options.limit {
            params.push(format!("limit={limit}"));
        }
        let mut path = format!("{}/memories", self.ws_prefix().await?);
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        let resp = self.transport().do_json(Method::GET, &path, None).await?;
        Ok(parse_list(&resp.data))
    }

    /// Every namespace in the workspace with its memory and chunk counts
    /// (`GET /v2/workspaces/{ws}/namespaces`).
    pub async fn namespaces(&self) -> Result<Vec<NamespaceInfo>, Error> {
        let path = format!("{}/namespaces", self.ws_prefix().await?);
        let resp = self.transport().do_json(Method::GET, &path, None).await?;
        Ok(parse_namespaces(&resp.data))
    }

    /// Deprecated wrapper over [`Client::list`]; kept for 0.2.x callers.
    #[deprecated(since = "0.3.0", note = "use `Client::list`")]
    #[allow(deprecated)]
    pub async fn list_recent(
        &self,
        options: Option<crate::types::ListRecentOptions>,
    ) -> Result<Vec<Value>, Error> {
        let options = options.unwrap_or_default();
        let page = self
            .list(Some(ListOptions {
                namespace: options.namespace,
                query: None,
                limit: options.n,
            }))
            .await?;
        Ok(page
            .memories
            .into_iter()
            .map(|item| serde_json::to_value(item).unwrap_or(Value::Null))
            .collect())
    }
}
