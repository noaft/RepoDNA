<div align="center">

# 🧬 RepoDNA

### *The Code Tells You **What**. The History Tells You **Why**.*

**RepoDNA** transforms your repository's commit history, pull requests, and code evolution into deep contextual knowledge — so developers and AI agents finally understand not just *what* the code does, but *why* every line exists.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](http://makeapullrequest.com)
[![Made with Love](https://img.shields.io/badge/Made%20with-Love-red.svg)](https://github.com)

</div>

---

## The Problem

When you open a codebase, your tools are great at telling you **what** the code does:

```python
if retries > MAX_RETRIES:
    raise CircuitBreakerException()
```

But none of them can tell you **why** this threshold is `MAX_RETRIES = 3` and not `5`. None of them know that this was added after a production outage at 2AM on a Friday. None of them remember that the original author debated this for two weeks in a PR review before settling on this value.

**That context — that *why* — is the most valuable knowledge in your codebase. And it's being lost every single day.**

---

## What is RepoDNA?

RepoDNA is a developer intelligence platform that **excavates the institutional knowledge buried in your git history** and makes it accessible, searchable, and understandable — for both humans and AI coding assistants.

Think of it as giving your codebase a long-term memory.

> Every commit is a decision. Every PR is a debate. Every revert is a lesson learned.
> RepoDNA makes sure none of that wisdom disappears.

---

## Core Features

### 🔍 Contextual Code Archaeology
Hover over any line of code and instantly see the full story behind it — the commit that introduced it, the bug it fixed, the PR discussion that shaped it, and the alternatives that were considered and rejected.

### 🧠 AI-Powered Intent Extraction
RepoDNA uses large language models to parse commit messages, PR descriptions, and code comments to extract **developer intent** — turning raw history into structured, queryable knowledge.

### 📜 Decision Timeline
Visualize the evolutionary history of any file, function, or module as a timeline. See how it changed, who changed it, why, and what triggered each major refactor.

### 🔗 Causal Chain Analysis
Understand cause-and-effect relationships across your codebase. When a piece of code was added to fix a bug — RepoDNA links it back to the issue, the failing test, and the incident report.

### 🤖 AI Agent Context Injection
Integrate RepoDNA with your AI coding assistants (GitHub Copilot, Cursor, Claude, etc.) to automatically inject historical context into every suggestion — so your AI agent understands the *why*, not just the *what*.

### 📊 Knowledge Decay Detection
Identify "orphaned knowledge" — code whose original authors have left, whose linked issues are closed, and whose context has never been documented. Before it becomes legacy debt.

### 🗺️ Architecture Decision Records (ADR) Auto-Generation
RepoDNA automatically reconstructs Architecture Decision Records from your commit history — even if you never wrote a single ADR.

---

## How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                        Your Repository                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐   ┌──────────────┐    │
│  │  Commits │  │    PRs   │  │  Issues  │   │   Comments   │    │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘   └──────┬───────┘    │
└───────┼─────────────┼─────────────┼────────────────┼────────────┘
        │             │             │                │
        └─────────────┴─────────────┴────────────────┘
                              │
                    ┌─────────▼─────────┐
                    │   RepoDNA Engine  │
                    │                   │
                    │  • Intent Mining  │
                    │  • Causal Linking │
                    │  • Context Graph  │
                    └─────────┬─────────┘
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
   ┌──────▼──────┐   ┌────────▼───────┐  ┌───────▼────────┐
   │  Dev Tools  │   │  AI Assistants │  │  Web Dashboard │
   │  (IDE ext.) │   │  (MCP / API)   │  │  (Analytics)   │
   └─────────────┘   └────────────────┘  └────────────────┘
```

1. **Ingest** — RepoDNA connects to your repository and ingests the full commit history, PR discussions, issue threads, and inline comments.
2. **Analyze** — The engine builds a semantic graph linking code changes to their motivations, authors, related issues, and temporal context.
3. **Surface** — Context is surfaced through IDE extensions, a web dashboard, and an API/MCP server that AI agents can query in real time.

---

## Use Cases

| Scenario | Without RepoDNA | With RepoDNA |
|---|---|---|
| **Onboarding a new developer** | Weeks of shadowing and tribal knowledge transfer | Hours of self-guided exploration with full context |
| **Debugging a mysterious bug** | "Who wrote this? Why is it like this?" | Instant causal chain from symptom to original decision |
| **AI code review** | AI suggests changes that violate past decisions | AI understands past decisions and respects constraints |
| **Code archaeology** | `git blame` + `git log` + grep + luck | Structured, semantic, queryable history |
| **Architecture review** | Reconstruct decisions from memory | Auto-generated ADRs with full evidence |
| **Refactoring safely** | Hope you don't break something invisible | Know exactly *why* the code is shaped the way it is |

---

## Getting Started

> **Note:** RepoDNA is in early development. Star and watch the repo to follow progress.

## Environment

RepoDNA reads optional environment settings from `src/settings.rs` and a sample file is included at `.env.example`.

- `REPODNA_DB_PATH`: store the SQLite database at one fixed file path.
- `REPODNA_HOME`: override the default RepoDNA storage root.

Default storage locations:

- Windows: `%LOCALAPPDATA%\RepoDNA`
- Unix-like systems: `~/.repodna`

```bash
# Clone the repository
git clone https://github.com/your-username/RepoDNA.git
cd RepoDNA

# Install dependencies
npm install   # or pip install -r requirements.txt

# Point RepoDNA at your repository
repodna analyze --repo /path/to/your/repo

# Start the dashboard
repodna serve
```

---

## Vision & Roadmap

---

## Why "RepoDNA"?

DNA is the blueprint of life. It doesn't just store what an organism is — it encodes *why* it evolved that way, the millions of years of decisions, pressures, and adaptations that shaped it.

Your repository is the same. Every commit is a mutation. Every PR is natural selection. Every revert is an extinction event.

**RepoDNA reads that blueprint — and makes the evolution legible.**

---

## Contributing

Contributions are what make the open source community such an amazing place. Any contributions you make are **greatly appreciated**.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

---

## License

Distributed under the MIT License. See `LICENSE` for more information.

---

<div align="center">

**Built for developers who believe code is not just written — it's *accumulated*.**

*Stop reading code. Start understanding it.*

</div>

