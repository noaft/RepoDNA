# Vision

## RepoDNA

RepoDNA is building the memory layer for code tools.

Today most coding systems are stateless by default. They can reason well inside a session, but they do not retain durable understanding of the repository once that session ends. The result is a constant loss of momentum:

- the same files are reopened
- the same architecture is rediscovered
- the same historical questions are asked again
- the same constraints are relearned
- the same partial insights die with the thread

Reasoning is improving fast.
Memory is still the bottleneck.

RepoDNA exists to fix that.

## Context Engineering

RepoDNA is a context engineering project.

The problem is not that Codex, Claude, or other coding tools cannot reason.
The problem is that every new session starts too close to zero. The model has
to reopen files, rebuild a mental map, rediscover why functions exist, and
guess which details from previous work still matter.

That repeated context rebuild is expensive in tokens, time, and quality. It
also makes agents act strangely: they may write code that is locally plausible
but disconnected from the repository's accumulated knowledge.

RepoDNA's job is to make useful context durable:

- what the source tree contains
- how code entities relate
- what changed recently
- which files and functions are risky
- what earlier sessions already learned
- what evidence supports a saved explanation

If context can be stored, retrieved, corrected, and reused, then coding tools
can spend more effort changing code and less effort rediscovering it.

## The Core Belief

The future of software tooling is not just better generation.

It is better continuity.

A strong coding tool should not only answer:

```text
What is in the repo right now?
```

It should also answer:

```text
Why is it like this?
What changed?
What usually breaks nearby?
Who knows this area?
What did we already learn in earlier sessions?
```

RepoDNA turns those questions into queryable state.

## Product Thesis

Every repository already contains a hidden memory system:

- commits
- diffs
- authorship
- call relationships
- file structure
- change patterns
- hotspots
- ownership signals

But that memory is fragmented across raw files, git logs, and human recollection.

RepoDNA restructures that raw material into a persistent knowledge graph that tools can search and traverse cheaply.

The goal is not to create yet another assistant.

The goal is to make every assistant less forgetful.

## Dogfood Principle

RepoDNA should be built with RepoDNA.

The first serious user is this repository itself. If a new development session
on RepoDNA still has to rediscover the same ingestion logic, MCP behavior,
storage rules, and product intent from scratch, the product is not working yet.

Every investigation should be an opportunity to improve durable memory:

- find the relevant node
- inspect the source when memory is missing
- save a concise explanation back to the graph
- let the next session retrieve it before reading broadly

This loop is the smallest proof that RepoDNA matters.

## The Job To Be Done

When a developer or coding agent returns to a repo, RepoDNA should help them recover context instead of rebuilding context.

That means:

- reconstructing the local map of the codebase
- surfacing historical intent behind the current shape
- preserving high-value relationships between code entities
- reducing repeated exploration across sessions
- providing a substrate that other tools can trust and query

If a session ends, the understanding should not end with it.

## What RepoDNA Should Become

At maturity, RepoDNA should sit beneath editors, agents, code review flows, and maintenance tooling as shared memory infrastructure.

```text
Repository activity
   ->
RepoDNA ingestion
   ->
Persistent knowledge graph
   ->
MCP / APIs / IDE integrations
   ->
Developers, agents, and automation
```

In that model:

- the repository is the source of truth
- RepoDNA is the memory engine
- tools become clients of durable context

The first client surface is MCP because it gives tools like Codex and Claude a
common way to ask for repository memory without each one needing a bespoke
integration.

## North Star Experience

One day, a user should be able to ask:

```text
Why does this code exist?
```

And get back something like:

```text
Purpose:
Prevent memory fragmentation in long-running GPU workers

Origin:
Introduced after production incident INC-421

Evidence:
Commit abc123
Related files scheduler.rs and allocator.rs

Tradeoff:
Higher allocation overhead, lower fragmentation risk

Risk:
Touches a hotspot owned by the runtime team
```

And a coding tool should be able to ask:

```text
What should I know before changing this area?
```

And receive a grounded answer derived from durable repository memory, not just the current prompt window.

## Why A Knowledge Graph

The problem is relational.

Files relate to functions.
Functions relate to commits.
Commits relate to authors.
Files co-change.
Hotspots emerge over time.
Architectural decisions leave traces across multiple artifacts.

A knowledge graph is a natural fit because it preserves those relationships instead of flattening everything into disconnected documents.

RepoDNA should make it easy to traverse:

- from a function to the files that contain it
- from a file to the commits that shaped it
- from a hotspot to the authors who repeatedly touched it
- from a bug fix to surrounding architectural consequences

## Strategic Position

RepoDNA is not the interface.
RepoDNA is the substrate.

It should be useful to:

- local coding agents
- MCP clients
- IDE extensions
- code review systems
- debugging workflows
- onboarding flows
- repository analytics tools

If assistants are the reasoning layer, RepoDNA should be the memory layer they stand on.

## Near-Term Focus

The immediate mission is practical:

1. Build a reliable local graph from repository history and structure.
2. Persist it in a form tools can reuse across sessions.
3. Treat every source file and code entity as a meaningful graph node.
4. Expose that graph through simple retrieval surfaces like MCP.
5. Let tools save useful context back onto graph nodes.
6. Make context recovery cheaper than context rediscovery.

This is the shortest path to proving the larger thesis.

## Long-Term Promise

Software teams should not have to keep paying the context tax.

Important architectural knowledge should not vanish because:

- the author left
- the issue is buried
- the session expired
- the thread was closed
- the agent forgot

RepoDNA's long-term promise is durable repository memory:

not just code search,
not just git history,
not just chat context,
but a living graph of what the code is, how it evolved, and why it matters.
