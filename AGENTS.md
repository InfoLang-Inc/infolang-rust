# infolang-rust — agent instructions

Official **Rust SDK** for InfoLang semantic memory. Wraps the public InfoLang
gateway REST API. Crate: `infolang` on crates.io. Async via `reqwest` +
`tokio`.

## Architecture

- `client.rs` — `Client`, `ClientBuilder`, credential/base-URL/namespace/workspace resolution.
- `transport.rs` — HTTP transport: retries (429 + 5xx), backoff with jitter, error mapping, metering headers.
- `auth.rs` — `ApiKeyAuth` (managed cloud) and `DevKeyAuth` (`key:namespace`, self-hosted).
- `memory.rs` / `context.rs` / `health.rs` — recall, investigate, remember, remember_batch, forget, banks, recent, context-pack, ingest, execute, stats, health.
- `types.rs` / `error.rs` — typed results and the error hierarchy.

## Contract

The authoritative REST contract is documented at
https://api.infolang.ai/docs. Verify request/response shapes against it,
never against assumptions.

## Rules

- Library-only — no CLI (Go owns the `infolang` binary).
- New endpoints: add the request builder + typed parser, the client method, and
  a wiremock integration test.
- Tests must stay offline by default; the live probe is gated by `INFOLANG_LIVE_TEST`.

## Commands

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```
