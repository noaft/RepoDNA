# Roadmap

## Phase 0 - Foundation

### Goal

Turn a repository into structured data.

No AI.
No MCP.
No Claude.

Input:

```text
Git Repository
```

Output:

```text
Knowledge Store
```

### Features

#### Git Scanner

Collect:

```text
Commit
Author
Date
Message
Diff
```

Example:

```json
{
  "commit": "abc123",
  "author": "John",
  "message": "Fix OOM issue",
  "files": ["scheduler.rs"]
}
```

#### AST Scanner

Collect:

```text
Module
Class
Function
Imports
```

Example:

```json
{
  "function": "allocate",
  "file": "cache.rs"
}
```

#### Dependency Scanner

Generate:

```text
Module A -> Module B
```

#### Call Graph

Generate:

```text
allocate()
    ↓
get_free_block()
    ↓
evict()
```

### Deliverable

```bash
repodna build .
```

Produces:

```text
.repodna/
commits.db
functions.db
dependencies.db
```

---

## Phase 1 - Repository Graph

### Goal

Build the first Knowledge Graph.

Entities:

```text
Repository
Module
File
Function
Class
Commit
Author
```

Relationships:

```text
calls
imports
modifies
introduced_in
depends_on
owns
```

Deliverable:

```bash
repodna graph
```

---

## Phase 2 - Repository Search

### Goal

Answer questions using the graph.

No AI.

Example:

```bash
repodna query "allocate"
```

Output:

```text
Function: allocate

Introduced:
commit abc123

Calls:
- get_free_block
- evict

Modified:
17 times
```

Deliverable:

```bash
repodna inspect allocate
```

---

## Phase 3 - Historical Intelligence

### Goal

Start understanding history.

This is where LLM usage begins.

Commit:

```text
Fix OOM issue
```

LLM output:

```json
{
  "type": "bug_fix",
  "category": "memory"
}
```

Commit:

```text
Replace FIFO with LRU
```

LLM output:

```json
{
  "type": "architecture_change",
  "from": "FIFO",
  "to": "LRU"
}
```

New entities:

```text
Decision
Incident
Refactor
Bug Fix
Feature
```

Deliverable:

```bash
repodna enrich
```

Produces:

```text
decisions.db
incidents.db
```

---

## Phase 4 - Architecture Intelligence

### Goal

Understand the repository like a Staff Engineer.

Module:

```text
cache
```

LLM output:

```json
{
  "purpose": "GPU memory management",
  "criticality": "high"
}
```

Module:

```text
scheduler
```

LLM output:

```json
{
  "purpose": "request orchestration"
}
```

Deliverable:

```bash
repodna architecture
```

Output:

```text
Cache Layer
Scheduler Layer
Execution Layer
Storage Layer
```

---

## Phase 5 - MCP Server

### Goal

Make it usable by agents.

```bash
repodna serve
```

Expose tools:

```text
get_function_info
get_commit_history
get_decision_history
get_module_summary
get_architecture_summary
```

Example:

```text
User: Optimize cache
Claude: RepoDNA says cache is high risk
```

Result: smarter code changes.

---

## Phase 6 - Agent Intelligence Layer

### Goal

Let agents reason using Repository Memory.

New tools:

```text
why_exists()
refactor_risk()
architectural_decisions()
incident_history()
ownership_info()
```

Before changing code:

```text
Why was this implemented?
```

RepoDNA answers with context.

---

## Long-Term Vision (2-3 years)

RepoDNA will not only read:

```text
Git
```

It will read:

```text
Git
PR
Issue
ADR
Notion
Slack
Linear
Jira
```

And generate:

```text
Repository Knowledge Graph
```

Positioning:

```text
GitHub Copilot -> reads code
RepoDNA -> understands the organization behind the code
```

---

## True MVP

Skip 90% of the ideas above and focus on one milestone:

```bash
repodna build .
```

Generate:

```text
Commit Graph
Call Graph
Dependency Graph
Ownership Graph
```

And run:

```bash
repodna inspect cache
```

To answer:

```text
Module: cache

Functions: 42

Modified: 317 times

Top Contributor: Alice

Dependencies:
- memory
- scheduler

Risk Level: High
```
