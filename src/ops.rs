use reqwest::Method;
use serde_json::{json, Value};

use crate::client::Client;
use crate::error::ConfigError;
use crate::transport::{Body, FilePart};
use crate::types::{parse_op_result, IngestFile, IngestJob, IngestOptions, OpResult, Operation};
use crate::Error;

impl Client {
    /// Batch primitives in one round trip
    /// (`POST /v2/workspaces/{ws}/execute` with the simple `{operations}`
    /// body — the gateway accepts it directly). One
    /// OpResult per sub-op, in order, inside `payload.results`.
    pub async fn execute(&self, operations: Vec<Operation>) -> Result<OpResult, Error> {
        let path = format!("{}/execute", self.ws_prefix().await?);
        let resp = self
            .transport()
            .do_json(
                Method::POST,
                &path,
                Some(json!({ "operations": operations })),
            )
            .await?;
        Ok(parse_op_result(&resp.data))
    }

    /// Server stats for the workspace (native OpResult envelope,
    /// `GET /v2/workspaces/{ws}/stats`).
    pub async fn stats(&self) -> Result<OpResult, Error> {
        let path = format!("{}/stats", self.ws_prefix().await?);
        let resp = self.transport().do_json(Method::GET, &path, None).await?;
        Ok(parse_op_result(&resp.data))
    }

    /// Encode text to an embedding (`POST /v2/workspaces/{ws}/encode`;
    /// response is the server payload, verbatim).
    pub async fn encode(&self, text: &str) -> Result<Value, Error> {
        let path = format!("{}/encode", self.ws_prefix().await?);
        let resp = self
            .transport()
            .do_json(Method::POST, &path, Some(json!({ "text": text })))
            .await?;
        Ok(resp.data)
    }

    /// Similarity between two texts (`POST /v2/workspaces/{ws}/similarity`;
    /// response is the server payload, verbatim).
    pub async fn similarity(&self, text1: &str, text2: &str) -> Result<Value, Error> {
        let path = format!("{}/similarity", self.ws_prefix().await?);
        let resp = self
            .transport()
            .do_json(
                Method::POST,
                &path,
                Some(json!({ "text1": text1, "text2": text2 })),
            )
            .await?;
        Ok(resp.data)
    }

    /// Ingest a zip of text content
    /// (`POST /v2/workspaces/{ws}/ingest?ns=&tag_prefix=`, raw zip bytes,
    /// `Content-Type: application/zip`). v1 is synchronous — the job returns
    /// finished.
    pub async fn ingest(
        &self,
        archive: Vec<u8>,
        options: Option<IngestOptions>,
    ) -> Result<IngestJob, Error> {
        let options = options.unwrap_or_default();
        let path = self.ingest_path(options).await?;
        let resp = self
            .transport()
            .request(
                Method::POST,
                &path,
                Body::Raw {
                    bytes: archive,
                    content_type: "application/zip".into(),
                },
            )
            .await?;
        decode_ingest(resp.data)
    }

    /// Ingest individual text files — multipart repeated `files` parts on the
    /// same ingest endpoint (the gateway dispatches on content type). No zip
    /// step needed.
    pub async fn ingest_files(
        &self,
        files: &[IngestFile],
        options: Option<IngestOptions>,
    ) -> Result<IngestJob, Error> {
        let options = options.unwrap_or_default();
        let path = self.ingest_path(options).await?;
        let parts = files
            .iter()
            .map(|file| FilePart {
                name: file.name.clone(),
                content: file.content.clone(),
            })
            .collect();
        let resp = self
            .transport()
            .request(Method::POST, &path, Body::Multipart(parts))
            .await?;
        decode_ingest(resp.data)
    }

    async fn ingest_path(&self, options: IngestOptions) -> Result<String, Error> {
        let mut params = Vec::new();
        if let Some(ns) = self.default_namespace(options.namespace) {
            params.push(format!("ns={}", urlencoding::encode(&ns)));
        }
        if let Some(tag_prefix) = options.tag_prefix {
            params.push(format!("tag_prefix={}", urlencoding::encode(&tag_prefix)));
        }
        let mut path = format!("{}/ingest", self.ws_prefix().await?);
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        Ok(path)
    }
}

fn decode_ingest(data: Value) -> Result<IngestJob, Error> {
    if !data.is_object() {
        return Ok(IngestJob::default());
    }
    serde_json::from_value(data).map_err(|err| {
        Error::Config(ConfigError {
            message: format!("failed to decode ingest response: {err}"),
        })
    })
}
