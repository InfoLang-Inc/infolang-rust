# Changelog

All notable changes to the InfoLang Rust SDK are documented here. This
project adheres to [Semantic Versioning](https://semver.org).

## [Unreleased]

## [0.3.0] - 2026-07-25

**BREAKING**: the SDK now targets the InfoLang gateway `/v2` API
(https://api.infolang.ai/docs), aligning with the TypeScript and Python SDKs
at 0.3.0 (this crate was never published, so the 0.1.0 → 0.3.0 jump carries
no semver debt). The legacy `/v1` endpoints this SDK called in
0.1.0 (`/v1/banks`, `/v1/context-pack`, `/v1/repos/{ns}/ingest`, the
bare-batch `/v1/execute`) no longer exist; `/v1` as a whole is deprecated.

### Added
- `whoami()` (`GET /v2/whoami`) and automatic workspace resolution: a
  credential with exactly one workspace grant needs no configuration;
  multi-workspace credentials pass `.workspace(...)` (or set
  `INFOLANG_WORKSPACE` / `INFOLANG_WORKSPACE_ID`). Workspace ids are opaque
  strings — never parsed client-side. Requests are path-scoped
  (`/v2/workspaces/{ws}/...`, percent-encoded); the
  `X-InfoLang-Workspace-Id` header model is gone. A failed resolution is not
  cached — a retry re-asks whoami.
- Recall retrieval extras: `golden`, `format`, `snippet_chars`, `adaptive`,
  `margin`.
  **Reserved:** these options are accepted today and activate in a
  future release — no SDK change needed.
- `list()` (gateway MemoryPage: `ns`/`q`/`limit`, `next_cursor`, search
  scores), `namespaces()` with per-namespace memory/chunk counts (bare
  string entries tolerated).
- `encode()`, `similarity()`; `execute()` now returns the
  `OpResult{ok, payload, error{code, message, capability}}` envelope from the
  simple `{operations}` batch body; `stats()` returns an `OpResult` too.
- `ingest()`: zip archives via `POST /v2/workspaces/{ws}/ingest?ns=&tag_prefix=`
  (raw bytes, `Content-Type: application/zip`).
- `ingest_files()`: multipart upload of individual text files (repeated
  `files` parts) on the same ingest endpoint — no zip step needed.
- `health()` now hits `GET /healthz`; new `ready()` (`GET /readyz`) — the
  authoritative readiness signal.
- Remember response fields surfaced: `stored`, `deduplicated`,
  `deduped_against`, `total_memories`; `X-InfoLang-Overage` surfaced as
  `Metering::overage`; `{error:{code,message}}` envelopes render as
  `"code: message"` and `ApiError::code` carries the machine-readable code.
- `memorize()` alias for `remember`.

### Changed
- `forget()` sends the namespace
  (`DELETE /v2/.../memories/{id}?namespace=`).
- `remember_batch()` is client-side sugar over `execute` with one real
  `remember` sub-op per item (the `remember_batch` pseudo-op is gone); empty
  input short-circuits without a request.
- Tags are always serialized as the array the API expects;
  comma-joined entries are split.
- The base URL defaults to `https://api.infolang.ai` ALWAYS — dev keys no
  longer redirect to the direct endpoint (`DIRECT_BASE_URL` remains as a
  deprecated const).
- `lane_not_supported` is **403** (was 401); both map to the authentication
  classifier, so no code change is needed by callers.

### Removed
- `list_banks()`, `context_pack()`, `ingest_repo()` and the context module —
  their endpoints no longer exist. `list_recent()` and `DevKeyAuth` remain
  as deprecated shims.

## [0.1.0] - Unreleased

### Added
- Initial release: async `Client` (tokio + reqwest/rustls) over the
  legacy `/v1` REST API: `recall`, `investigate`, `remember`,
  `remember_batch`, `forget`, `list_banks`, `list_recent`, `context_pack`,
  `ingest_repo`, `execute`, `stats`, `health`.
- Auth providers: API key, dev key (`key:namespace`).
- Typed error hierarchy, automatic retries with jitter, metering metadata.
