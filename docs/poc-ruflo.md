# ruflo POC Evaluation

Evaluation of [ruflo](https://github.com/ruvnet/ruflo) (v3.5.42) — an enterprise
AI orchestration platform native to Claude Code — within the council Rust/gRPC
workspace.

## Installation

```bash
npm install --save-dev ruflo   # adds ~861 packages
npx ruflo init --force --non-interactive
npx ruflo swarm init --v3-mode
```

Initialization scaffolds 115 files across `.claude-flow/`, `.claude/skills/`,
`.claude/agents/`, `.claude/commands/`, and `.claude/helpers/`. A `.mcp.json`
is generated for MCP server integration.

## What ruflo provides

| Capability | Detail |
|---|---|
| **Agent orchestration** | 99 agent definitions across 23 categories (architecture, testing, devops, etc.) |
| **Swarm coordination** | Hierarchical-mesh topology with up to 15 concurrent agents, auto-scaling, consensus coordination |
| **Q-Learning router** | Routes tasks to agents using Q-Learning with mixture-of-experts; starts with exploration, improves over time |
| **Self-learning hooks** | 26 hook types (pre/post edit, pre/post command, intelligence, analytics) that integrate via Claude Code's hook system |
| **Memory system** | Hybrid backend with HNSW vector indexing, semantic search, memory graph with PageRank |
| **MCP server** | Exposes ruflo capabilities as MCP tools for Claude Code |
| **Skills & commands** | 30 skills and 10 command groups for Claude Code |
| **Neural patterns** | Pattern recognition and Flash Attention integration |

## POC Results

### Doctor check (10 pass, 4 warnings)

```
✓ Version Freshness: v3.5.42 (up to date)
✓ Node.js Version: v24.6.0 (>= 20 required)
✓ Claude Code CLI: v2.1.81
✓ Git Repository: In a git repository
✓ Config File: Found
✓ MCP Servers: 1 configured (ruflo)
✓ agentic-flow: v2.0.7 (ReasoningBank, Router, QUIC)
⚠ Daemon Status: Not running (expected — not started)
⚠ Memory Database: Not initialized (SQLite not required for eval)
⚠ API Keys: No API keys found (expected — no LLM calls in POC)
⚠ TypeScript: Not installed locally (JS runtime sufficient)
```

### Task routing

Routing "implement a new gRPC endpoint for agent health checks" correctly
selected the **Architect** agent. Q-values start at 0 (cold start) and improve
with use via reinforcement learning.

### Swarm initialization

Successfully created a hierarchical-mesh swarm with consensus coordination,
auto-scaling, and Flash Attention enabled.

### Hooks integration

ruflo writes Claude Code hooks into `.claude/settings.json`:
- **PreToolUse**: intercepts `Bash` and `Write|Edit|MultiEdit` calls
- **PostToolUse**: post-processing for edits and commands
- **Notification**: coordination hooks

All hooks delegate to `.claude/helpers/hook-handler.cjs`.

### MCP server configuration

Generated `.mcp.json` configures a `claude-flow` MCP server that exposes
ruflo orchestration as tools within Claude Code sessions.

## Integration with council workspace

| Aspect | Assessment |
|---|---|
| **Coexistence** | ruflo's files live under `.claude-flow/`, `.claude/`, and `.mcp.json` — orthogonal to the Rust/gRPC crates. No conflicts with `Cargo.toml` or existing source. |
| **Build system** | Adds `package.json` with a single dev dependency. Does not interfere with `cargo build`. |
| **Git footprint** | Most generated files are already gitignored (`.claude/` except settings). Runtime data stays in `.claude-flow/data/`, `.claude-flow/logs/`, `.claude-flow/sessions/`. |
| **Settings conflict** | ruflo overwrites `.claude/settings.json` with its hooks. Teams sharing this file should merge carefully. |
| **Squad coexistence** | ruflo and Squad occupy different directories (`.claude-flow/` vs `.squad/`). Both can coexist, but their hook/settings philosophies differ — Squad uses Copilot skills while ruflo uses Claude Code hooks. |

## Limitations & risks

1. **Cold-start routing** — Q-Learning router starts with zero knowledge;
   all agents score equally until enough tasks provide feedback.
2. **Heavy scaffolding** — 115 generated files, 861 npm packages. Most are
   agent/skill templates that may never be used.
3. **Settings ownership** — ruflo takes full ownership of `.claude/settings.json`
   hooks. Manual hook entries will be overwritten on `ruflo init`.
4. **AgentDB patch warning** — every command prints a controller-not-found
   warning (cosmetic, non-blocking).
5. **Node.js dependency** — adds a Node.js runtime requirement to an otherwise
   pure-Rust project.
6. **Daemon requirement** — full agent orchestration requires the ruflo daemon
   running in the background (`ruflo daemon start`).

## Recommendation

ruflo is a comprehensive orchestration platform that could add value for
complex multi-agent workflows. For the council project specifically:

- **Useful if** the team plans to extend council with AI-assisted development
  workflows, automated code review pipelines, or multi-agent task
  decomposition.
- **Overkill if** the primary use is deliberation sessions via the existing
  daemon/CLI architecture, where the 5-agent council model is sufficient.

The MCP server integration and Q-Learning router are the most promising
features for potential integration. The self-learning hooks could complement
council's own hook system for development-time automation.

## Files added

| Path | Purpose |
|---|---|
| `package.json` | ruflo dev dependency |
| `.mcp.json` | MCP server configuration for Claude Code |
| `.claude-flow/config.yaml` | ruflo V3 runtime configuration |
| `.claude-flow/` | Runtime data, hooks, learning, metrics, sessions |
| `CLAUDE.md` | Claude Code behavioral rules (ruflo-generated) |
| `docs/poc-ruflo.md` | This evaluation document |
