# OpenAI-Compatible Embeddings Design

## Goal

Allow RepoDNA to use multiple embedding models through an OpenAI-compatible embeddings API while keeping the current local Nomic model as the default.

## Design

RepoDNA will add embedding settings in `src/settings.rs`:

- `REPODNA_EMBEDDING_PROVIDER`: defaults to `nomic`; accepts `nomic` or `openai`.
- `REPODNA_EMBEDDING_MODEL`: optional model name. For `openai`, defaults to `text-embedding-3-small`. For `nomic`, the model remains `nomic-ai/nomic-embed-text-v1.5`.
- `OPENAI_API_KEY`: required when provider is `openai`.
- `OPENAI_BASE_URL`: optional OpenAI-compatible API base URL, defaulting to `https://api.openai.com/v1`.

`src/embeddings.rs` will expose a small embedding client that returns both the vector and the model id used to produce it. The Nomic path stays local-first and cached with `OnceCell`; the OpenAI-compatible path sends `POST /embeddings` with `{ "model": "...", "input": "..." }`.

`src/bin/repodna_mcp.rs` will store the returned model id in `function_summary_embeddings.model` instead of the hardcoded Nomic model. Tests will use fake embedders so they do not call the network or load local models.

## Error Handling

Empty input remains rejected before any model initialization or HTTP call. OpenAI-compatible embedding will fail with actionable errors when the API key is missing, the response is not successful, or no vector is returned.

## Testing

Unit tests will cover settings defaults, OpenAI env parsing, model metadata persistence, and empty input behavior. Network-backed embedding tests will not run by default.
