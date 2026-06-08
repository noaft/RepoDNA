# RepoDNA :brain: :dna:

> **Persistent memory for coding tools**
>
> Stop losing repository context every time the session resets.

RepoDNA gives coding tools a long-term memory.

Most code tools are brilliant inside the current window and forgetful the moment the session ends. They can read files, patch code, and answer local questions, but they repeatedly lose the deeper context:

- Why does this function exist?
- Which bug or incident forced this design?
- What files usually change together?
- Which author or subsystem owns this area?
- What did the last agent already discover before the session died?

RepoDNA turns repository history and code structure into a persistent knowledge graph so developers and coding agents can recover context instead of recomputing it from scratch every time.

## The Story :repeat:

### The Context Tax

Every coding session starts the same way:

1. The tool reads the current files.
2. It forms a temporary mental model.
3. It makes progress.
4. The session ends.
5. That mental model disappears.

Then the next session pays the tax again:

- reopen the same files
- rerun the same searches
- rederive the same architecture
- rediscover the same historical constraints
- reask the same "why is this here?" questions

RepoDNA exists to break that loop.

Instead of letting insight evaporate with the session, RepoDNA stores durable context in a graph:

- code entities such as files, directories, and functions
- historical evidence such as commits and authors
- relationships such as contains, modifies, calls, and co-change patterns
- metadata such as hotspots, ownership, and active symbols

The result is a memory layer that tools can query quickly through local APIs and MCP.

## What RepoDNA Is :card_index_dividers:

### Not Another Assistant. The Memory Underneath.

RepoDNA is a repository knowledge engine that builds and serves a persistent graph for humans and AI coding tools.

Think of it as:

- a memory cache for code understanding
- a local knowledge graph for repository context
- a bridge between raw git history and agent-friendly retrieval

RepoDNA is not trying to replace your coding assistant.

RepoDNA is the memory system underneath the assistant.

## Why It Matters :zap:

### Better Continuity, Less Rediscovery

Without persistent memory, coding tools are forced to infer everything from the current snapshot. That makes them:

- repetitive
- fragile across session boundaries
- weak at preserving architectural intent
- expensive in tokens and time
- prone to suggesting changes that ignore historical constraints

With RepoDNA, a tool can ask better questions:

- Show me active functions matching `build_graph`.
- Which files are hotspots in this subsystem?
- Which commits introduced this behavior?
- What other files usually change with this one?
- Which author has the strongest ownership signal here?

That means less rediscovery and more continuity.

## Core Idea :spider_web:

### Turn Repo Activity Into Durable Context

RepoDNA builds a knowledge graph from two inputs:

1. Repository history
2. Repository structure

From there it exposes durable context to tools.

```text
Repository
   ->
RepoDNA ingestion
   ->
Knowledge graph
   ->
MCP / local API / future integrations
   ->
Developers and coding agents
```

## Current Capabilities :hammer_and_wrench:

### Foundation First

Today RepoDNA focuses on the foundation layer:

- ingest git history into a local SQLite-backed graph
- extract repository nodes such as commits, authors, files, directories, and functions
- compute relationships like modifies, contains, calls, and co-change
- calculate ownership and hotspot metadata
- expose graph-backed search through an MCP server

This is the groundwork for persistent repository memory.

## Environment :gear:

### Shared Storage For Shared Memory

RepoDNA reads optional environment settings from [src/settings.rs](/abs/path/d:/Git/RepoDNA/src/settings.rs:1). A sample file is included at [.env.example](/abs/path/d:/Git/RepoDNA/.env.example:1).

- `REPODNA_DB_PATH`: store the SQLite database at one fixed file path
- `REPODNA_HOME`: override the default RepoDNA storage root

Default storage locations:

- Windows: `%LOCALAPPDATA%\RepoDNA`
- Unix-like systems: `~/.repodna`

## Getting Started :rocket:

### From Repo To Retrieval

Build the knowledge graph for a repository:

```powershell
cargo run -- build D:\Git\RepoDNA
```

Use a fixed database path if you want tools to share one durable store:

```powershell
$env:REPODNA_DB_PATH='D:\RepoDNA\.repodna\graph.db'
cargo run -- build D:\Git\RepoDNA
```

Start the MCP server:

```powershell
$env:REPODNA_DB_PATH='D:\RepoDNA\.repodna\graph.db'
cargo run --bin repodna_mcp -- D:\Git\RepoDNA
```

Register it with Codex:

```powershell
codex mcp add repo_dna --env REPODNA_DB_PATH=D:\RepoDNA\.repodna\graph.db -- cargo run --bin repodna_mcp -- D:\Git\RepoDNA
```

## What Success Looks Like :dart:

### The Return-To-Context Test

The long-term goal is simple:

When a developer or agent returns to a codebase after minutes, days, or weeks away, they should not have to start from zero.

They should be able to recover:

- what exists
- why it exists
- what changed
- who changed it
- what is risky
- what the last useful line of investigation already discovered

RepoDNA helps turn code understanding from a per-session activity into a persistent asset.

## Vision :telescope:

### Why This Matters Long Term

The broader product direction lives in [docs/VISION.md](/abs/path/d:/Git/RepoDNA/docs/VISION.md:1).

Short version:

- coding tools need memory, not just reasoning
- repositories need durable context, not just source snapshots
- knowledge graphs are a practical substrate for preserving that context

## Contributing :handshake:

### Build The Memory Layer

Contributions are welcome. If you want to help, start by improving one of these layers:

- ingestion quality
- graph schema
- retrieval quality
- MCP ergonomics
- developer workflows around persistent repository memory

## License :page_facing_up:

Distributed under the MIT License. See `LICENSE` for more information.

Built for teams who are tired of paying the context tax every session.

> **RepoDNA helps code tools remember what the repo already knows.**
