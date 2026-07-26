use infolang::{
    Client, ForgetOptions, IngestFile, IngestOptions, InvestigateOptions, ListOptions,
    RecallOptions, RememberItem, RememberOptions, CLOUD_BASE_URL, WEAK_SCORE_FLOOR,
};
use serial_test::serial;
use wiremock::matchers::{body_json, header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn clear_env() {
    for key in [
        "INFOLANG_API_KEY",
        "INFOLANG_DEV_KEY",
        "INFOLANG_BASE_URL",
        "INFOLANG_NAMESPACE",
        "INFOLANG_WORKSPACE",
        "INFOLANG_WORKSPACE_ID",
    ] {
        std::env::remove_var(key);
    }
}

/// Client with an explicit workspace — never calls whoami.
fn client_for(server: &MockServer) -> Client {
    Client::builder()
        .api_key("il_live_test")
        .base_url(server.uri())
        .workspace("ws-123")
        .namespace("docs")
        .build()
        .expect("client")
}

/// Client with no workspace configured — resolves via whoami.
fn client_auto(server: &MockServer) -> Client {
    Client::builder()
        .api_key("il_live_test")
        .base_url(server.uri())
        .namespace("docs")
        .build()
        .expect("client")
}

fn whoami_body(ids: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "lane": "managed_api",
        "principal": "prn_1",
        "workspaces": ids
            .iter()
            .map(|id| serde_json::json!({ "workspace_id": id, "role": "admin" }))
            .collect::<Vec<_>>()
    })
}

// --- recall ----------------------------------------------------------------

#[tokio::test]
#[serial]
async fn investigate_sends_recall_with_top_k_and_headers() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .and(header("Authorization", "Bearer il_live_test"))
        .and(header("User-Agent", "infolang-rust/0.3.0"))
        .and(body_json(serde_json::json!({
            "query": "auth middleware",
            "namespace": "docs",
            "top_k": 5
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "namespace": "docs",
            "latency_ms": 4.2,
            "hits": [{ "id": "m1", "text": "auth middleware", "tags": "auth", "similarity": 0.91 }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client
        .investigate("auth middleware", None)
        .await
        .expect("investigate");
    assert_eq!(result.chunks.len(), 1);
    assert_eq!(result.chunks[0].text, "auth middleware");
    assert_eq!(result.chunks[0].score, Some(0.91));
    assert_eq!(result.namespace.as_deref(), Some("docs"));
    assert_eq!(result.latency_ms, Some(4.2));
    assert!(!result.weak());
}

#[tokio::test]
#[serial]
async fn recall_parses_verbose_hit_fields_and_weak() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "namespace": "docs",
            "hits": [{
                "id": "h1",
                "text": "hit text",
                "tags": "t1,t2",
                "similarity": 0.5,
                "source": "notes.md",
                "timestamp": 1720000000.0
            }]
        })))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client
        .recall(
            "query",
            Some(RecallOptions {
                verbose: Some(true),
                top_k: Some(3),
                ..Default::default()
            }),
        )
        .await
        .expect("recall");
    assert!(result.weak());
    let chunk = &result.chunks[0];
    assert_eq!(chunk.id, "h1");
    assert_eq!(chunk.score, Some(0.5));
    assert_eq!(chunk.tags.as_deref(), Some("t1,t2"));
    assert_eq!(chunk.source.as_deref(), Some("notes.md"));
    assert_eq!(chunk.timestamp, Some(1720000000.0));
}

#[tokio::test]
#[serial]
async fn recall_serializes_extras_only_when_some() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .and(body_json(serde_json::json!({
            "query": "q",
            "namespace": "docs",
            "top_k": 2,
            "verbose": true,
            "golden": true,
            "format": "snippet",
            "snippet_chars": 240,
            "adaptive": true,
            "margin": 0.07
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "hits": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    client
        .recall(
            "q",
            Some(RecallOptions {
                top_k: Some(2),
                verbose: Some(true),
                golden: Some(true),
                format: Some("snippet".into()),
                snippet_chars: Some(240),
                adaptive: Some(true),
                margin: Some(0.07),
                ..Default::default()
            }),
        )
        .await
        .expect("recall with extras");
}

#[tokio::test]
#[serial]
async fn recall_omits_extras_when_none() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .and(body_json(serde_json::json!({
            "query": "q",
            "namespace": "docs"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "hits": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    client.recall("q", None).await.expect("bare recall");
}

#[tokio::test]
#[serial]
async fn workspace_id_is_percent_encoded_in_paths() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws%2F1/recall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "hits": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder()
        .api_key("il_live_test")
        .base_url(server.uri())
        .workspace("ws/1")
        .build()
        .expect("client");
    client.recall("q", None).await.expect("recall");
}

// --- remember / forget -----------------------------------------------------

#[tokio::test]
#[serial]
async fn remember_sends_tags_array_and_parses_dedup_fields() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/remember"))
        .and(body_json(serde_json::json!({
            "text": "note",
            "namespace": "docs",
            "source": "test",
            "tags": ["a", "b", "c"]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "memory_id": "mem-1",
            "namespace": "docs",
            "stored": false,
            "deduplicated": true,
            "deduped_against": "mem-0",
            "total_memories": 10
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let remembered = client
        .remember(
            "note",
            Some(RememberOptions {
                source: Some("test".into()),
                // Comma-joined entries are split into the array the API expects.
                tags: Some(vec!["a".into(), "b, c".into()]),
                ..Default::default()
            }),
        )
        .await
        .expect("remember");
    assert_eq!(remembered.memory_id.as_deref(), Some("mem-1"));
    assert_eq!(remembered.stored, Some(false));
    assert_eq!(remembered.deduplicated, Some(true));
    assert_eq!(remembered.deduped_against.as_deref(), Some("mem-0"));
    assert_eq!(remembered.total_memories, Some(10));
}

#[tokio::test]
#[serial]
async fn forget_sends_namespace_query() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/workspaces/ws-123/memories/mem-1"))
        .and(query_param("namespace", "docs"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    client.forget("mem-1", None).await.expect("forget");
}

#[tokio::test]
#[serial]
async fn forget_omits_namespace_when_unset() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v2/workspaces/ws-123/memories/mem-2"))
        .and(query_param_is_missing("namespace"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder()
        .api_key("il_live_test")
        .base_url(server.uri())
        .workspace("ws-123")
        .build()
        .expect("client");
    client
        .forget("mem-2", Some(ForgetOptions { namespace: None }))
        .await
        .expect("forget without namespace");
}

// --- list / namespaces -----------------------------------------------------

#[tokio::test]
#[serial]
async fn list_builds_query_and_parses_page() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/workspaces/ws-123/memories"))
        .and(query_param("ns", "docs"))
        .and(query_param("q", "auth"))
        .and(query_param("limit", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "memories": [
                {
                    "id": "m1",
                    "text": "auth notes",
                    "namespace": "docs",
                    "source": "notes.md",
                    "tags": ["auth", "http"],
                    "timestamp": "2026-07-25T00:00:00Z",
                    "score": 0.93
                },
                { "id": "m2", "text": "other", "namespace": "docs" }
            ],
            "nextCursor": "cur-2"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let page = client
        .list(Some(ListOptions {
            query: Some("auth".into()),
            limit: Some(2),
            ..Default::default()
        }))
        .await
        .expect("list");
    assert_eq!(page.memories.len(), 2);
    assert_eq!(
        page.memories[0].tags.as_deref(),
        Some(&["auth".to_string(), "http".to_string()][..])
    );
    assert_eq!(page.memories[0].score, Some(0.93));
    assert_eq!(page.memories[1].source, None);
    assert_eq!(page.next_cursor.as_deref(), Some("cur-2"));
}

#[tokio::test]
#[serial]
async fn namespaces_tolerates_bare_strings() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/workspaces/ws-123/namespaces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "namespaces": [
                { "namespace": "docs", "memories": 3, "chunks": 9 },
                "plain"
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let namespaces = client.namespaces().await.expect("namespaces");
    assert_eq!(namespaces.len(), 2);
    assert_eq!(namespaces[0].namespace, "docs");
    assert_eq!(namespaces[0].memories, Some(3));
    assert_eq!(namespaces[0].chunks, Some(9));
    assert_eq!(namespaces[1].namespace, "plain");
    assert_eq!(namespaces[1].memories, None);
}

#[tokio::test]
#[allow(deprecated)]
#[serial]
async fn list_recent_deprecated_wraps_list() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/workspaces/ws-123/memories"))
        .and(query_param("ns", "docs"))
        .and(query_param("limit", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "memories": [{ "id": "m1", "text": "t", "namespace": "docs" }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let recent = client
        .list_recent(Some(infolang::ListRecentOptions {
            n: Some(1),
            ..Default::default()
        }))
        .await
        .expect("recent");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0]["id"], "m1");
}

// --- encode / similarity / execute / stats --------------------------------

#[tokio::test]
#[serial]
async fn encode_and_similarity_post_expected_bodies() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/encode"))
        .and(body_json(serde_json::json!({ "text": "hi" })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "embedding": [0.1, 0.2] })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/similarity"))
        .and(body_json(serde_json::json!({ "text1": "a", "text2": "b" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "similarity": 0.42 })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let encoded = client.encode("hi").await.expect("encode");
    assert_eq!(encoded["embedding"][1], 0.2);
    let sim = client.similarity("a", "b").await.expect("similarity");
    assert_eq!(sim["similarity"], 0.42);
}

#[tokio::test]
#[serial]
async fn execute_sends_simple_operations_body_and_parses_op_result() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/execute"))
        .and(body_json(serde_json::json!({
            "operations": [{ "op": "stats" }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "payload": { "results": [{ "ok": true, "payload": { "total": 1 } }] }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client
        .execute(vec![infolang::Operation {
            op: "stats".into(),
            args: None,
            select: None,
        }])
        .await
        .expect("execute");
    assert!(result.ok);
    assert_eq!(
        result.payload.expect("payload")["results"][0]["payload"]["total"],
        1
    );
    assert!(result.error.is_none());
}

#[tokio::test]
#[serial]
async fn stats_parses_op_result_error() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/workspaces/ws-123/stats"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": false,
            "error": { "code": "capability_required", "message": "no", "capability": "stats" }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.stats().await.expect("stats");
    assert!(!result.ok);
    let err = result.error.expect("op error");
    assert_eq!(err.code, "capability_required");
    assert_eq!(err.capability.as_deref(), Some("stats"));
}

// --- ingest ----------------------------------------------------------------

#[tokio::test]
#[serial]
async fn ingest_sends_zip_bytes_with_content_type_and_query() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/ingest"))
        .and(query_param("ns", "docs"))
        .and(query_param("tag_prefix", "repo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "status": "finished", "chunks": 3 })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let archive = b"PK\x03\x04zipbytes".to_vec();
    let job = client
        .ingest(
            archive.clone(),
            Some(IngestOptions {
                tag_prefix: Some("repo".into()),
                ..Default::default()
            }),
        )
        .await
        .expect("ingest");
    assert_eq!(job.status.as_deref(), Some("finished"));
    assert_eq!(job.extra["chunks"], 3);

    let requests = server.received_requests().await.expect("recording");
    let req = &requests[0];
    assert_eq!(
        req.headers
            .get("content-type")
            .expect("content-type")
            .to_str()
            .unwrap(),
        "application/zip"
    );
    assert_eq!(req.body, archive);
}

#[tokio::test]
#[serial]
async fn ingest_files_sends_multipart_parts() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/ingest"))
        .and(query_param("ns", "docs"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": "finished" })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let job = client
        .ingest_files(
            &[
                IngestFile {
                    name: "a.md".into(),
                    content: b"alpha content".to_vec(),
                },
                IngestFile {
                    name: "b.md".into(),
                    content: b"beta content".to_vec(),
                },
            ],
            None,
        )
        .await
        .expect("ingest_files");
    assert_eq!(job.status.as_deref(), Some("finished"));

    let requests = server.received_requests().await.expect("recording");
    let req = &requests[0];
    let content_type = req
        .headers
        .get("content-type")
        .expect("content-type")
        .to_str()
        .unwrap();
    assert!(content_type.starts_with("multipart/form-data"));
    let body = String::from_utf8_lossy(&req.body);
    assert_eq!(body.matches("name=\"files\"").count(), 2);
    assert!(body.contains("filename=\"a.md\""));
    assert!(body.contains("filename=\"b.md\""));
    assert!(body.contains("alpha content"));
    assert!(body.contains("beta content"));
}

// --- workspace resolution --------------------------------------------------

#[tokio::test]
#[serial]
async fn explicit_workspace_skips_whoami() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(whoami_body(&["ws-auto"])))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "hits": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    client.recall("q", None).await.expect("recall");
}

#[tokio::test]
#[serial]
async fn single_grant_resolves_via_whoami_once_and_caches() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(whoami_body(&["ws-auto"])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-auto/recall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "hits": [] })))
        .expect(2)
        .mount(&server)
        .await;

    let client = client_auto(&server);
    client.recall("q1", None).await.expect("first recall");
    client.recall("q2", None).await.expect("second recall");
}

#[tokio::test]
#[serial]
async fn multi_grant_error_names_candidate_ids() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(whoami_body(&["ws-a", "ws-b"])))
        .mount(&server)
        .await;

    let client = client_auto(&server);
    let err = client.recall("q", None).await.unwrap_err();
    let message = err.to_string();
    assert!(matches!(err, infolang::Error::Config(_)), "{message}");
    assert!(message.contains("ws-a"), "{message}");
    assert!(message.contains("ws-b"), "{message}");
    assert!(message.contains("INFOLANG_WORKSPACE"), "{message}");
}

#[tokio::test]
#[serial]
async fn zero_grants_error() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(whoami_body(&[])))
        .mount(&server)
        .await;

    let client = client_auto(&server);
    let err = client.recall("q", None).await.unwrap_err();
    let message = err.to_string();
    assert!(matches!(err, infolang::Error::Config(_)), "{message}");
    assert!(message.contains("no visible workspace grants"), "{message}");
}

#[tokio::test]
#[serial]
async fn failed_resolution_is_not_cached() {
    clear_env();
    let server = MockServer::start().await;
    // First whoami answers with two grants (resolution fails), then with one.
    Mock::given(method("GET"))
        .and(path("/v2/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(whoami_body(&["ws-a", "ws-b"])))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(whoami_body(&["ws-auto"])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-auto/recall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "hits": [] })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_auto(&server);
    let err = client.recall("q", None).await.unwrap_err();
    assert!(matches!(err, infolang::Error::Config(_)));
    // The failure was not cached: the retry re-asks whoami and succeeds.
    client
        .recall("q", None)
        .await
        .expect("recall after re-resolve");
}

#[tokio::test]
#[serial]
async fn whoami_parses_identity() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "lane": "managed_api",
            "principal": null,
            "workspaces": [{ "workspace_id": "ws-1", "role": "reader", "plan": "pro" }],
            "scopes": ["memory:read"]
        })))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let who = client.whoami().await.expect("whoami");
    assert_eq!(who.lane, "managed_api");
    assert_eq!(who.principal, None);
    assert_eq!(who.workspaces[0].workspace_id, "ws-1");
    assert_eq!(who.workspaces[0].role, "reader");
    assert_eq!(who.workspaces[0].extra["plan"], "pro");
    assert_eq!(
        who.scopes.as_deref(),
        Some(&["memory:read".to_string()][..])
    );
}

// --- remember_batch --------------------------------------------------------

#[tokio::test]
#[serial]
async fn remember_batch_builds_remember_sub_ops() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/execute"))
        .and(body_json(serde_json::json!({
            "operations": [
                {
                    "op": "remember",
                    "args": { "text": "one", "source": "fallback", "namespace": "docs" }
                },
                {
                    "op": "remember",
                    "args": {
                        "text": "two",
                        "source": "own",
                        "tags": ["tag", "extra"],
                        "namespace": "docs"
                    }
                }
            ]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "payload": {
                "results": [
                    { "ok": true, "payload": { "id": "a", "namespace": "docs", "stored": true, "total_memories": 1 } },
                    { "ok": true, "payload": { "id": "b", "namespace": "docs", "stored": true, "total_memories": 2 } }
                ]
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let results = client
        .remember_batch(
            &[
                RememberItem {
                    text: "one".into(),
                    tags: None,
                    source: None,
                },
                RememberItem {
                    text: "two".into(),
                    tags: Some(vec!["tag, extra".into()]),
                    source: Some("own".into()),
                },
            ],
            Some(RememberOptions {
                source: Some("fallback".into()),
                ..Default::default()
            }),
        )
        .await
        .expect("batch");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].memory_id.as_deref(), Some("a"));
    assert_eq!(results[1].memory_id.as_deref(), Some("b"));
    assert_eq!(results[1].total_memories, Some(2));
}

#[tokio::test]
#[serial]
async fn remember_batch_empty_short_circuits() {
    clear_env();
    let server = MockServer::start().await;
    let client = client_for(&server);
    let out = client.remember_batch(&[], None).await.expect("empty batch");
    assert!(out.is_empty());
    let requests = server.received_requests().await.expect("recording");
    assert!(requests.is_empty());
}

// --- errors & metering -----------------------------------------------------

#[tokio::test]
#[serial]
async fn error_envelope_renders_code_message() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("x-request-id", "req-404")
                .set_body_json(serde_json::json!({
                    "error": { "code": "not_found", "message": "memory missing" }
                })),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client.recall("missing", None).await.unwrap_err();
    assert!(err.is_not_found());
    match err {
        infolang::Error::Api(api) => {
            assert_eq!(api.status, 404);
            assert_eq!(api.message, "not_found: memory missing");
            assert_eq!(api.code.as_deref(), Some("not_found"));
            assert_eq!(api.request_id, "req-404");
        }
        other => panic!("expected api error, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn flat_error_body_still_maps() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/remember"))
        .respond_with(
            ResponseTemplate::new(422).set_body_json(serde_json::json!({ "detail": "bad tags" })),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client.remember("x", None).await.unwrap_err();
    assert!(err.is_validation());
    match err {
        infolang::Error::Api(api) => {
            assert_eq!(api.message, "bad tags");
            assert_eq!(api.code, None);
        }
        other => panic!("expected api error, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn lane_not_supported_403_is_authentication() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/remember"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "error": { "code": "lane_not_supported", "message": "session lane cannot use v2" }
        })))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client.remember("x", None).await.unwrap_err();
    assert!(err.is_authentication());
    match err {
        infolang::Error::Api(api) => {
            assert_eq!(api.code.as_deref(), Some("lane_not_supported"));
        }
        other => panic!("expected api error, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn rate_limit_exposes_retry_after() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "1.5")
                .set_body_json(serde_json::json!({
                    "error": { "code": "rate_limited", "message": "slow down" }
                })),
        )
        .mount(&server)
        .await;

    let client = Client::builder()
        .api_key("il_live_test")
        .base_url(server.uri())
        .workspace("ws-123")
        .max_retries(0)
        .build()
        .expect("client");
    let err = client.recall("q", None).await.unwrap_err();
    assert!(err.is_rate_limit());
    match err {
        infolang::Error::Api(api) => {
            assert_eq!(api.retry_after, 1.5);
            assert_eq!(api.code.as_deref(), Some("rate_limited"));
        }
        other => panic!("expected api error, got {other:?}"),
    }
}

#[tokio::test]
#[serial]
async fn overage_metering_header_is_parsed() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-infolang-overage", "3")
                .insert_header("x-infolang-tokens-saved", "12")
                .set_body_json(serde_json::json!({ "hits": [] })),
        )
        .mount(&server)
        .await;

    let client = client_for(&server);
    let result = client.recall("q", None).await.expect("recall");
    let metering = result.metering.expect("metering");
    assert_eq!(metering.overage, Some(3));
    assert_eq!(metering.tokens_saved, Some(12));
}

// --- health ----------------------------------------------------------------

#[tokio::test]
#[serial]
async fn healthz_and_readyz() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": "ok" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/readyz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ready": true,
            "ml_ready": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server);
    let health = client.health().await.expect("health");
    assert_eq!(health["status"], "ok");
    let ready = client.ready().await.expect("ready");
    assert_eq!(ready["ml_ready"], true);
}

// --- retries ---------------------------------------------------------------

#[tokio::test]
#[serial]
async fn retry_on_503_then_success() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "hits": [] })))
        .mount(&server)
        .await;

    let client = Client::builder()
        .api_key("il_live_test")
        .base_url(server.uri())
        .workspace("ws-123")
        .max_retries(2)
        .build()
        .expect("client");
    let result = client
        .recall("query", None)
        .await
        .expect("recall after retry");
    assert!(result.chunks.is_empty());
}

#[tokio::test]
#[serial]
async fn retry_on_429_honors_retry_after() {
    clear_env();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/workspaces/ws-123/recall"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "hits": [] })))
        .mount(&server)
        .await;

    let client = Client::builder()
        .api_key("il_live_test")
        .base_url(server.uri())
        .workspace("ws-123")
        .max_retries(1)
        .build()
        .expect("client");
    client.recall("query", None).await.expect("recall");
}

// --- configuration ---------------------------------------------------------

#[test]
#[serial]
fn env_resolution_and_dev_key() {
    clear_env();
    std::env::set_var("INFOLANG_API_KEY", "il_live_env");
    std::env::set_var("INFOLANG_NAMESPACE", "envns");
    std::env::set_var("INFOLANG_WORKSPACE", "ws-env");
    std::env::set_var("INFOLANG_BASE_URL", "https://example.test");
    let client = Client::builder().build().expect("client");
    assert_eq!(client.base_url, "https://example.test");
    assert_eq!(client.namespace, "envns");
    assert_eq!(client.workspace.as_deref(), Some("ws-env"));

    clear_env();
    std::env::set_var("INFOLANG_API_KEY", "il_live_env");
    std::env::set_var("INFOLANG_WORKSPACE_ID", "ws-id");
    let client = Client::builder().build().expect("client");
    assert_eq!(client.workspace.as_deref(), Some("ws-id"));

    clear_env();
    std::env::set_var("INFOLANG_DEV_KEY", "secret:devns");
    let client = Client::builder().build().expect("dev client");
    // 0.3.0: dev keys still pin the namespace but no longer redirect to a
    // direct endpoint — the gateway is ALWAYS the default base URL.
    assert_eq!(client.base_url, CLOUD_BASE_URL);
    assert_eq!(client.namespace, "devns");

    clear_env();
    let client = Client::new("il_live_abc").expect("api client");
    assert_eq!(client.base_url, CLOUD_BASE_URL);
    assert!(client.workspace.is_none());
}

#[test]
fn weak_score_floor_constant() {
    assert!((WEAK_SCORE_FLOOR - 0.85).abs() < f64::EPSILON);
}

#[test]
fn investigate_default_top_k() {
    let opts = InvestigateOptions::default();
    assert_eq!(opts.top_k, None);
}
