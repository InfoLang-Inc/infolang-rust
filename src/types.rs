use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Confidence floor below which the top recall match is considered weak.
pub const WEAK_SCORE_FLOOR: f64 = 0.85;

/// A single recalled memory (normalized from the `hits[]` wire shape).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    /// Memory id (wire: `id`). Opaque — do not parse.
    pub id: String,
    /// Cosine similarity (wire: `similarity`).
    pub score: Option<f64>,
    /// Memory text. Empty when `format: "meta"` was requested.
    pub text: String,
    /// Comma-joined tags; present only when the memory has tags.
    pub tags: Option<String>,
    /// Origin label; present only on `verbose: true` recalls.
    pub source: Option<String>,
    /// Epoch seconds; present only on `verbose: true` recalls.
    pub timestamp: Option<f64>,
}

/// Usage metadata parsed from gateway response headers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metering {
    pub tokens_saved: Option<i64>,
    pub chunks_used: Option<i64>,
    pub repo_coverage: Option<f64>,
    /// Overage units incurred by this call, when the plan is over its
    /// inclusion (`X-InfoLang-Overage`).
    pub overage: Option<i64>,
    pub request_id: String,
}

/// Result of a recall or investigate call.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecallResult {
    pub chunks: Vec<Chunk>,
    pub namespace: Option<String>,
    /// Server-observed latency in milliseconds, when reported.
    pub latency_ms: Option<f64>,
    pub metering: Option<Metering>,
}

impl RecallResult {
    /// True when the top match scores below the 0.85 confidence floor.
    pub fn weak(&self) -> bool {
        self.chunks
            .first()
            .and_then(|chunk| chunk.score)
            .map(|score| score < WEAK_SCORE_FLOOR)
            .unwrap_or(false)
    }
}

/// Result of a remember or memorize call (server shape).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RememberResult {
    pub memory_id: Option<String>,
    pub namespace: Option<String>,
    /// False when the write was absorbed by a near-duplicate (never billed).
    pub stored: Option<bool>,
    /// Present and true only on a dedup absorb.
    pub deduplicated: Option<bool>,
    /// The id of the existing memory that absorbed this write.
    pub deduped_against: Option<String>,
    pub total_memories: Option<i64>,
}

/// One memory row from `list` (gateway MemoryPage item).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryItem {
    pub id: String,
    pub text: String,
    pub namespace: String,
    pub source: Option<String>,
    pub tags: Option<Vec<String>>,
    pub timestamp: Option<String>,
    /// Relevance score; present only on search (`q`) results.
    pub score: Option<f64>,
}

/// A page of memories; `next_cursor` is absent on the last page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryPage {
    pub memories: Vec<MemoryItem>,
    pub next_cursor: Option<String>,
}

/// A namespace with its logical memory count and chunk row count.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NamespaceInfo {
    pub namespace: String,
    pub memories: Option<i64>,
    pub chunks: Option<i64>,
}

/// A workspace grant visible to the current credential.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WhoamiWorkspace {
    /// Opaque workspace id.
    pub workspace_id: String,
    pub role: String,
    /// Scopes that actually work on this workspace, when scoping is active.
    pub scopes: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Identity echo for the current credential (`GET /v2/whoami`).
///
/// Kept structurally open: the gateway adds fields (scope sources, credential
/// descriptors) without notice, and ids are opaque.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Whoami {
    pub lane: String,
    pub principal: Option<String>,
    pub workspaces: Vec<WhoamiWorkspace>,
    pub scopes: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// Result of an ingest call (`POST /v2/workspaces/{ws}/ingest`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IngestJob {
    pub status: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// One file for a multipart `ingest_files` upload.
#[derive(Debug, Clone, PartialEq)]
pub struct IngestFile {
    pub name: String,
    pub content: Vec<u8>,
}

/// The native OpResult envelope (execute and stats).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OpResult {
    pub ok: bool,
    pub payload: Option<Value>,
    pub error: Option<OpError>,
}

/// Machine-readable error inside an [`OpResult`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpError {
    pub code: String,
    pub message: String,
    pub capability: Option<String>,
}

/// Single entry in an execute batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<Vec<String>>,
}

/// One entry passed to remember_batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RememberItem {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RecallOptions {
    pub namespace: Option<String>,
    pub top_k: Option<u32>,
    pub verbose: Option<bool>,
    /// Token-optimal preset (see the recall docs). Reserved.
    pub golden: Option<bool>,
    /// Hit text rendering: `full | lossless | exact | snippet | meta`.
    pub format: Option<String>,
    /// Window size in characters for `format: "snippet"`.
    pub snippet_chars: Option<u64>,
    /// Trim hits past a similarity cliff.
    pub adaptive: Option<bool>,
    /// Similarity margin used by the adaptive cutoff.
    pub margin: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct InvestigateOptions {
    pub namespace_hint: Option<String>,
    pub top_k: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct RememberOptions {
    pub namespace: Option<String>,
    pub source: Option<String>,
    /// Always serialized as an array; entries containing commas are split.
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct ForgetOptions {
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    pub namespace: Option<String>,
    /// Semantic search instead of a listing; results carry `score`.
    pub query: Option<String>,
    pub limit: Option<u32>,
}

/// Options for the deprecated [`crate::Client::list_recent`] wrapper.
#[deprecated(since = "0.3.0", note = "use `ListOptions` with `Client::list`")]
#[derive(Debug, Clone, Default)]
pub struct ListRecentOptions {
    pub namespace: Option<String>,
    pub n: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct IngestOptions {
    /// Target namespace (default `default`).
    pub namespace: Option<String>,
    /// Extra tag applied to every ingested chunk.
    pub tag_prefix: Option<String>,
}

// --- wire parsing (tolerant, mirrors the TypeScript SDK's parsers) ---------

pub(crate) fn parse_recall(data: &Value, metering: Option<Metering>) -> RecallResult {
    let hits = data
        .get("hits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let chunks = hits
        .iter()
        .map(|hit| Chunk {
            id: hit
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            score: hit.get("similarity").and_then(|v| v.as_f64()),
            text: hit
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            tags: hit.get("tags").and_then(|v| v.as_str()).map(str::to_string),
            source: hit
                .get("source")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            timestamp: hit.get("timestamp").and_then(|v| v.as_f64()),
        })
        .collect();
    RecallResult {
        chunks,
        namespace: data
            .get("namespace")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        latency_ms: data.get("latency_ms").and_then(|v| v.as_f64()),
        metering,
    }
}

pub(crate) fn parse_remember(data: &Value) -> RememberResult {
    let id = data
        .get("id")
        .and_then(|v| v.as_str())
        .or_else(|| data.get("memory_id").and_then(|v| v.as_str()));
    RememberResult {
        memory_id: id.map(str::to_string),
        namespace: data
            .get("namespace")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        stored: data.get("stored").and_then(|v| v.as_bool()),
        deduplicated: match data.get("deduplicated").and_then(|v| v.as_bool()) {
            Some(true) => Some(true),
            _ => None,
        },
        deduped_against: data
            .get("deduped_against")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        total_memories: data.get("total_memories").and_then(|v| v.as_i64()),
    }
}

pub(crate) fn parse_list(data: &Value) -> MemoryPage {
    let memories = data
        .get("memories")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .map(|item| serde_json::from_value(item.clone()).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    MemoryPage {
        memories,
        next_cursor: data
            .get("nextCursor")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }
}

pub(crate) fn parse_namespaces(data: &Value) -> Vec<NamespaceInfo> {
    data.get("namespaces")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    if let Some(name) = item.as_str() {
                        return NamespaceInfo {
                            namespace: name.to_string(),
                            memories: None,
                            chunks: None,
                        };
                    }
                    NamespaceInfo {
                        namespace: item
                            .get("namespace")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        memories: item.get("memories").and_then(|v| v.as_i64()),
                        chunks: item.get("chunks").and_then(|v| v.as_i64()),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_op_result(data: &Value) -> OpResult {
    OpResult {
        ok: data.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
        payload: data.get("payload").filter(|v| !v.is_null()).cloned(),
        error: data
            .get("error")
            .filter(|v| !v.is_null())
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
    }
}

/// One `RememberResult` per sub-op, in order, from an execute envelope.
pub(crate) fn parse_execute_remember_batch(data: &Value) -> Vec<RememberResult> {
    let envelope = parse_op_result(data);
    envelope
        .payload
        .as_ref()
        .and_then(|payload| payload.get("results"))
        .and_then(|v| v.as_array())
        .map(|results| {
            results
                .iter()
                .map(|result| parse_remember(result.get("payload").unwrap_or(&Value::Null)))
                .collect()
        })
        .unwrap_or_default()
}

/// Splits comma-joined entries, trims, and drops empties — the API
/// expects tags as an array.
pub(crate) fn normalize_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .flat_map(|tag| tag.split(','))
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_recall_maps_hits() {
        let data = json!({
            "namespace": "docs",
            "latency_ms": 12.5,
            "hits": [
                { "id": "m1", "text": "auth", "similarity": 0.91, "tags": "a,b",
                  "source": "notes", "timestamp": 1000.0 }
            ]
        });
        let result = parse_recall(&data, None);
        assert_eq!(result.chunks[0].id, "m1");
        assert_eq!(result.chunks[0].score, Some(0.91));
        assert_eq!(result.chunks[0].tags.as_deref(), Some("a,b"));
        assert_eq!(result.chunks[0].source.as_deref(), Some("notes"));
        assert_eq!(result.chunks[0].timestamp, Some(1000.0));
        assert_eq!(result.namespace.as_deref(), Some("docs"));
        assert_eq!(result.latency_ms, Some(12.5));
        assert!(!result.weak());
    }

    #[test]
    fn recall_weak_when_low_or_missing_score() {
        let low = parse_recall(
            &json!({ "hits": [{ "id": "m1", "text": "t", "similarity": 0.5 }] }),
            None,
        );
        assert!(low.weak());
        let scoreless = parse_recall(&json!({ "hits": [{ "id": "m1", "text": "t" }] }), None);
        assert!(!scoreless.weak());
        let empty = parse_recall(&json!({ "hits": [] }), None);
        assert!(!empty.weak());
    }

    #[test]
    fn parse_remember_reads_server_fields() {
        let result = parse_remember(&json!({
            "memory_id": "m1",
            "namespace": "docs",
            "stored": false,
            "deduplicated": true,
            "deduped_against": "m0",
            "total_memories": 4
        }));
        assert_eq!(result.memory_id.as_deref(), Some("m1"));
        assert_eq!(result.stored, Some(false));
        assert_eq!(result.deduplicated, Some(true));
        assert_eq!(result.deduped_against.as_deref(), Some("m0"));
        assert_eq!(result.total_memories, Some(4));
        // `deduplicated: false` is normalized to None, like the TS SDK.
        assert_eq!(
            parse_remember(&json!({ "id": "m2", "deduplicated": false })).deduplicated,
            None
        );
    }

    #[test]
    fn parse_namespaces_tolerates_bare_strings() {
        let out = parse_namespaces(&json!({
            "namespaces": [
                { "namespace": "docs", "memories": 3, "chunks": 9 },
                "plain"
            ]
        }));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].memories, Some(3));
        assert_eq!(out[1].namespace, "plain");
        assert_eq!(out[1].memories, None);
    }

    #[test]
    fn parse_execute_remember_batch_maps_sub_payloads() {
        let out = parse_execute_remember_batch(&json!({
            "ok": true,
            "payload": {
                "results": [
                    { "ok": true, "payload": { "id": "a", "stored": true } },
                    { "ok": true, "payload": { "id": "b", "stored": true } }
                ]
            }
        }));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].memory_id.as_deref(), Some("a"));
        assert_eq!(out[1].memory_id.as_deref(), Some("b"));
    }

    #[test]
    fn normalize_tags_splits_and_trims() {
        let tags = vec!["a, b".to_string(), "c".to_string(), " ".to_string()];
        assert_eq!(normalize_tags(&tags), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_op_result_reads_error() {
        let result = parse_op_result(&json!({
            "ok": false,
            "error": { "code": "capability_required", "message": "no", "capability": "stats" }
        }));
        assert!(!result.ok);
        let err = result.error.expect("error");
        assert_eq!(err.code, "capability_required");
        assert_eq!(err.capability.as_deref(), Some("stats"));
    }
}
