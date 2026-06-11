# OpenAI-Compatible Embeddings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add OpenAI-compatible embedding configuration while preserving the local Nomic default.

**Architecture:** Settings owns env parsing. `embeddings.rs` owns provider selection and request execution. MCP receives embedding results with model metadata and persists that model id.

**Tech Stack:** Rust 2024, `anyhow`, `fastembed`, `once_cell`, `rusqlite`, standard library HTTP or a minimal HTTP client dependency if needed.

---

### Task 1: Settings

**Files:**
- Modify: `src/settings.rs`

- [ ] Write failing tests for default `nomic` settings and OpenAI-compatible env parsing.
- [ ] Run `cargo test settings`.
- [ ] Implement `EmbeddingProvider`, `EmbeddingSettings`, and env parsing.
- [ ] Re-run `cargo test settings`.

### Task 2: Embedding Client

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/embeddings.rs`

- [ ] Write failing tests for empty input and model metadata.
- [ ] Run `cargo test embeddings`.
- [ ] Implement an `EmbeddingResult { model, vector }` return type.
- [ ] Preserve existing `embed_text_with_nomic` compatibility.
- [ ] Add OpenAI-compatible request support.
- [ ] Re-run `cargo test embeddings`.

### Task 3: MCP Persistence

**Files:**
- Modify: `src/bin/repodna_mcp.rs`

- [ ] Write failing test showing fake embedder model metadata is stored.
- [ ] Run `cargo test --bin repodna_mcp`.
- [ ] Update add-context flow to persist the embedder-provided model id.
- [ ] Re-run `cargo test --bin repodna_mcp`.

### Task 4: Operator Docs

**Files:**
- Modify: `.env.example`
- Modify: `README.md`

- [ ] Document `REPODNA_EMBEDDING_PROVIDER`, `REPODNA_EMBEDDING_MODEL`, `OPENAI_API_KEY`, and `OPENAI_BASE_URL`.
- [ ] Run `cargo check`.
