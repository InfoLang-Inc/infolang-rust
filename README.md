# infolang (Rust)

Official async Rust client for [InfoLang](https://infolang.ai) semantic memory.
It wraps the InfoLang gateway `/v2` API (https://api.infolang.ai/docs) with an
idiomatic client, typed errors, automatic retries, and workspace/namespace
scoping.

- Async via `reqwest` + `tokio`.
- Crate: `infolang` (crates.io).

## Install

Add to your `Cargo.toml`:

```toml
[dependencies]
infolang = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Or:

```bash
cargo add infolang
```

## Quickstart

```rust
use infolang::Client;

#[tokio::main]
async fn main() -> Result<(), infolang::Error> {
    let client = Client::new("il_live_...")?; // or set INFOLANG_API_KEY and use Client::builder().build()?
    let result = client
        .investigate("how does auth middleware work?", None)
        .await?;
    for chunk in result.chunks {
        println!("{:.3}  {}", chunk.score.unwrap_or(0.0), chunk.text);
    }
    Ok(())
}
```

## Scoping

Workspace = tenant, namespace = memory bank. Requests are path-scoped
(`/v2/workspaces/{workspace_id}/...`); workspace ids are opaque strings.
A credential with exactly one workspace grant needs no configuration — the
SDK resolves it via `GET /v2/whoami` on first use and caches it.
Multi-workspace credentials pass `workspace` (or set `INFOLANG_WORKSPACE`):

```rust
use infolang::Client;

let client = Client::builder()
    .api_key("il_live_...")
    .namespace("docs")
    .workspace("<workspace id>") // optional for single-grant credentials
    .build()?;
```

## Authentication

| Mode | Construction | Default target |
|------|--------------|----------------|
| Managed cloud (API key) | `Client::new("il_live_...")` | `https://api.infolang.ai` |

A managed API key honors the client `namespace` on both reads and writes.
`dev_key` support remains as a deprecated shim (it still pins the namespace,
but no longer redirects to a direct endpoint — the gateway is always the
default base URL).

Environment fallbacks (when the corresponding builder field is unset):

- `INFOLANG_API_KEY`, `INFOLANG_DEV_KEY`
- `INFOLANG_BASE_URL`, `INFOLANG_NAMESPACE`
- `INFOLANG_WORKSPACE` or `INFOLANG_WORKSPACE_ID`

## API surface

All workspace-scoped paths below are prefixed with `/v2/workspaces/{ws}`.

| Method | HTTP | Notes |
|--------|------|-------|
| `whoami` | `GET /v2/whoami` | Identity echo + workspace grants |
| `recall` | `POST .../recall` | Semantic recall |
| `investigate` | `POST .../recall` | Agent-style recall (default top-k 5) |
| `remember` / `memorize` | `POST .../remember` | Store a memory (tags sent as an array) |
| `remember_batch` | `POST .../execute` | Client-side sugar: one `remember` sub-op per item |
| `forget` | `DELETE .../memories/{id}?namespace=` | Delete by id |
| `list` | `GET .../memories?ns=&q=&limit=` | List or semantically search memories |
| `namespaces` | `GET .../namespaces` | Per-namespace memory/chunk counts |
| `encode` | `POST .../encode` | Text → embedding |
| `similarity` | `POST .../similarity` | Similarity between two texts |
| `execute` | `POST .../execute` | Batch operations (`OpResult` envelope) |
| `stats` | `GET .../stats` | Store statistics (`OpResult` envelope) |
| `ingest` | `POST .../ingest?ns=&tag_prefix=` | Zip archive (`application/zip`) |
| `ingest_files` | `POST .../ingest?ns=&tag_prefix=` | Multipart `files` parts, no zip step |
| `health` | `GET /healthz` | Liveness |
| `ready` | `GET /readyz` | Readiness — the authoritative signal |

Recall retrieval extras (`golden`, `format`, `snippet_chars`, `adaptive`,
`margin`) are serialized only when set. **Reserved:** accepted today and
activated in a future release.

## Errors

Failures return `infolang::Error`:

- `Error::Config` — missing credentials, workspace-resolution failure, or
  decode failure
- `Error::Connection` — transport/timeout failure
- `Error::Api` — non-2xx gateway response (`status`, `code`, `request_id`,
  `body`); gateway `{error:{code,message}}` envelopes render as
  `"code: message"`

Use helper methods: `is_authentication()` (401/403 — `lane_not_supported`
is 403), `is_not_found()`, `is_validation()`, `is_rate_limit()`
(`retry_after`), `is_server()`.

## Development

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
INFOLANG_LIVE_TEST=1 cargo test --test live_test -- --ignored
```

## License

Apache-2.0 — see [LICENSE](LICENSE).
