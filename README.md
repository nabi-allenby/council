# Council

A turn-based deliberation system where AI agents with distinct personalities discuss a question across rounds, then cast binding votes.

Five default roles — Architect, Sentinel, Steward, Mediator, Firebrand — each with unique reasoning styles, blind spots, and interpersonal dynamics.

## Quick Start

```bash
# Install
pip install -e .

# Set your API key
echo 'ANTHROPIC_API_KEY=your-key-here' > .env

# Run
python -m council "Should we rewrite the backend in Rust?"
```

## CLI Options

```
python -m council <question> [options]

  --verbose, -v          Show turns and votes during execution
  --rounds, -r INT       Discussion rounds, 1-3 (default: 3)
  --model, -m NAME       Model for all agents (default: claude-haiku-4-5-20251001)
  --rotation AGENTS      Comma-separated agent order, e.g. architect,sentinel,steward
  --tools PAIRS          Agent:tool pairs, e.g. architect:web_search
  --backend, -b BACKEND  Agent backend: 'api' or 'agent-sdk' (default: api)
```

## Configuration

Edit `agents/council.json`:

```json
{
  "rotation": ["architect", "sentinel", "steward", "mediator", "firebrand"],
  "rounds": 3,
  "model": "claude-haiku-4-5-20251001",
  "backend": "api",
  "tools": { "architect": ["web_search"] }
}
```

**Constraints:** 1-7 agents (odd count, or exactly 1), 1-3 rounds. Each agent needs a matching `agents/<name>.md` personality file.

### Backend Options

| Backend | Description | Install |
|---------|-------------|---------|
| `api` (default) | Direct Anthropic Messages API. Lightweight, single-shot calls. | `pip install -e .` |
| `agent-sdk` | Claude Agent SDK. Multi-turn tool use, MCP server support, built-in retries. | `pip install -e '.[agent-sdk]'` |

Both backends use the same personality files, config, and structured output validation. The `agent-sdk` backend is best when agents need richer tool use (web search with follow-up queries, MCP integrations).

## Project Structure

```
agents/          Agent personality files (.md) and council.json config
council/         Core library (orchestrator, agent, config, schema, prompt, report)
prompts/         Round-specific and format prompt templates
tests/           Test suite (pip install -e '.[dev]' for pytest)
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
pip install -e '.[dev]'
pytest tests/
```
