# Council

A turn-based deliberation system where AI agents with distinct personalities discuss a question across rounds, then cast binding votes.

Five default roles — Architect, Sentinel, Steward, Mediator, Firebrand — each with unique reasoning styles, blind spots, and interpersonal dynamics.

## Quick Start

```bash
# Build
cargo build --release

# Run (uses Claude CLI via agent-sdk backend by default)
./target/release/council "Should we rewrite the backend in Rust?"

# Or with the direct API backend
export ANTHROPIC_API_KEY='your-key-here'
./target/release/council -b api "Should we rewrite the backend in Rust?"
```

## CLI Options

```
council <question> [options]

  --verbose, -v          Show turns and votes during execution
  --rounds, -r INT       Discussion rounds, 1-3 (default: 3)
  --model, -m NAME       Model for all agents (default: claude-haiku-4-5-20251001)
  --rotation AGENTS      Comma-separated agent order, e.g. architect,sentinel,steward
  --tools PAIRS          Agent:tool pairs, e.g. architect:web_search
  --backend, -b BACKEND  Agent backend: 'agent-sdk' or 'api' (default: agent-sdk)
```

## Configuration

Edit `agents/council.json`:

```json
{
  "rotation": ["architect", "sentinel", "steward", "mediator", "firebrand"],
  "rounds": 3,
  "model": "claude-haiku-4-5-20251001",
  "backend": "agent-sdk",
  "tools": { "architect": ["web_search"] }
}
```

**Constraints:** 1-7 agents (odd count, or exactly 1), 1-3 rounds. Each agent needs a matching `agents/<name>.md` personality file.

### Backend Options

| Backend | Description |
|---------|-------------|
| `agent-sdk` (default) | Calls the `claude` CLI in print mode. No API key needed. |
| `api` | Direct Anthropic Messages API. Requires `ANTHROPIC_API_KEY`. |

Both backends use the same personality files, config, and structured output validation.

## Project Structure

```
agents/          Agent personality files (.md) and council.json config
src/             Rust source (orchestrator, agent, config, schema, prompt, report)
prompts/         Round-specific and format prompt templates
tests/           Integration tests (cargo test)
docs/            In-depth research and design documentation
logs/            Generated session reports (gitignored)
```

## Worktree Integration

New issues automatically receive an "Open workspace" comment link via the [Worktree](https://worktree.io/) GitHub Action. Clicking it creates an isolated git worktree on your local machine and opens it in your editor.

To use this, install the Worktree daemon locally:

```bash
cargo install worktree-io
worktree setup
```

## Tests

```bash
cargo test
```
