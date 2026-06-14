# Roadmap

RepoDNA is a context engineering system for coding tools.

The product is not another assistant. The product is durable repository memory that Codex, Claude, editors, review tools, and future agents can all query through local APIs and MCP.

The roadmap is organized around one practical test:

> Can a new session recover useful repo context faster than rebuilding it from scratch?

## Now - Dogfood Memory Loop

### Goal

Make RepoDNA useful for its own development.

Every RepoDNA session should start with less rediscovery than the previous one. When an agent investigates a function, file, or subsystem, the useful result should become durable graph context that the next session can retrieve.

### Build

- reliable `build` and `update` flows for the current worktree
- full source-tree ingestion: every indexed file becomes a `File` node
- Rust semantic extraction for functions, structs, traits, globals, calls, main flow
- saved function context through MCP
- graph API and HTML viewer for manual inspection

### Success Criteria

- Codex can use RepoDNA memory before broad filesystem search
- a saved function summary is retrievable in a later session
- non-Rust files appear as meaningful `File` nodes in the graph
- `cargo check` remains the baseline verification command

## Next - Whole Source Graph

### Goal

Treat every source node as meaningful.

RepoDNA should not feel like a narrow function index. Files, directories, functions, symbols, and relationships should form one coherent source graph.

### Build

- stable node ids for files, directories, and code symbols
- source-tree relationships such as `CONTAINS`
- code relationships such as `CALLS`, `MAIN_TREE`, and `MAIN_FLOW`
- graph queries that work across node types, not only functions
- viewer defaults that show the source graph directly

### Success Criteria

- a user can search for a file, function, directory, or symbol and land on the right node
- the graph can answer "what is near this node?"
- non-semantic files remain simple `File` nodes instead of fake parsed entities
- function-level memory keeps working while broader graph memory expands

## Next - MCP As The Product Surface

### Goal

Make RepoDNA easy for coding tools to consume.

MCP is the first real product surface because it lets Codex, Claude, and other tools share the same repository memory layer without custom integrations.

### Build

- keep existing function memory tools stable
- add graph-wide MCP tools such as:
  - `search_nodes`
  - `search_files`
  - `get_node_neighbors`
  - `get_node_context`
  - `add_node_context`
- keep MCP startup quiet and resilient on stdio
- keep outputs schema-safe with object root schemas

### Success Criteria

- a coding agent can ask RepoDNA for context before reading files broadly
- MCP can retrieve file, directory, and function nodes
- tool results are small enough to be useful inside model context
- failures are actionable rather than looking like handshake bugs

## Next - Durable Context Engineering

### Goal

Let agents write down what they learned.

RepoDNA should preserve more than raw graph structure. It should store durable explanations, summaries, observations, and evidence that survive session boundaries.

### Build

- node-attached summaries and descriptions
- provenance for saved context: source node, author/tool, timestamp, evidence
- update flows for stale context
- retrieval ranking that combines graph proximity, saved summaries, and lexical/embedding search

### Success Criteria

- useful investigation results can be saved without editing prompt files
- future sessions can distinguish durable context from raw metadata
- stale or wrong context can be corrected through MCP
- saved context reduces repeated token spend in real dogfood sessions

## Later - Change-Aware Planning

### Goal

Help agents plan changes with repo memory.

Before modifying code, an agent should be able to ask RepoDNA what matters around the intended change: related files, hotspots, ownership, previous context, and likely risk.

### Build

- git diff ingestion for working changes
- changed-node detection
- nearby context packs for affected nodes
- risk hints based on hotspots, ownership, and graph relationships
- plan-oriented MCP tools

### Success Criteria

- given a diff, RepoDNA can identify impacted graph nodes
- agents can request "what should I know before changing this?"
- generated plans cite graph evidence instead of relying only on prompt memory

## Later - Team Memory Layer

### Goal

Make repository memory useful beyond one machine and one session.

RepoDNA should stay local-first, but teams should be able to share memory intentionally when they want to.

### Build

- predictable storage with `REPODNA_HOME` and `REPODNA_DB_PATH`
- export/import of graph memory
- optional sync strategy
- repo-scoped memory boundaries
- documentation for Codex, Claude, and local workflows

### Success Criteria

- multiple tools can point at the same repository memory
- storage behavior is predictable across CLI, API, and MCP
- shared memory does not require a cloud dependency by default

## Long-Term Direction

RepoDNA can eventually ingest more than git and source files:

- pull requests
- issues
- ADRs
- release notes
- incident reports
- docs and design notes
- team knowledge systems

But the order matters.

The first durable win is local repository memory that helps coding tools stop paying the same context tax every session.

## True MVP

The true MVP is not a large feature list.

It is this loop:

```text
Build graph
  ->
Use Codex or Claude through MCP
  ->
Save useful context to graph
  ->
Start a new session
  ->
Recover that context without rediscovery
```

If that loop works, RepoDNA is doing its job.
