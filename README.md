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
```

## Configuration

Edit `agents/council.json`:

```json
{
  "rotation": ["architect", "sentinel", "steward", "mediator", "firebrand"],
  "rounds": 3,
  "model": "claude-haiku-4-5-20251001",
  "tools": { "architect": ["web_search"] }
}
```

**Constraints:** 1-7 agents (odd count, or exactly 1), 1-3 rounds. Each agent needs a matching `agents/<name>.md` personality file.

## Project Structure

```
agents/          Agent personality files (.md) and council.json config
council/         Core library (orchestrator, agent, config, schema, prompt, report)
prompts/         Round-specific and format prompt templates
tests/           Test suite (pip install -e '.[dev]' for pytest)
docs/            In-depth research and design documentation
logs/            Generated session reports (gitignored)
```

## Tests

```bash
pip install -e '.[dev]'
pytest tests/
```
