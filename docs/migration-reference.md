# Migration Reference: Python to Rust

Module boundaries, data flow, and type mappings for porting the council
system from Python to Rust.

## Module Map

| Python module | Rust target | Purpose |
|---|---|---|
| `types.py` | `src/types.rs` | Core data types: `Turn`, `Vote`, `Session`, `ParsedResponse`, `ParsedVote` |
| `schema.py` | `src/schema.rs` | JSON parsing + validation of `---RESPONSE---` and `---VOTE---` blocks |
| `config.py` | `src/config.rs` | Load and validate `agents/council.json` |
| `prompt.py` | `src/prompt.rs` | Load and compose prompt templates from `prompts/` |
| `agent_base.py` | `src/agent.rs` (trait) | `AgentBackend` trait, shared retry constants, text normalization |
| `agent.py` | `src/api_backend.rs` | Direct Anthropic API backend (implements `AgentBackend`) |
| `agent_sdk_backend.py` | deferred | Agent SDK backend (evaluate Rust SDK availability) |
| `orchestrator.py` | `src/orchestrator.rs` | Session runner: discussion rounds + vote phase |
| `output.py` | `src/output.rs` | Concise decision record for stdout |
| `report.py` | `src/report.rs` | Full markdown report generation + file writing |
| `__main__.py` | `src/main.rs` | CLI entry point (argparse -> clap) |

## Data Flow

```
CLI (main.rs)
  |
  v
load_config() -> CouncilConfig
  |
  v
Orchestrator::new(config)
  |-- creates AgentBackend instances per rotation role
  |-- loads personality files from agents/*.md
  |
  v
Orchestrator::run(question) -> Session
  |
  |-- for each round 1..N:
  |     for each agent in rotation:
  |       1. discussion_prompt(round, total) -> system context
  |       2. _build_transcript(session, round, role) -> user message
  |       3. agent.respond(round, system, messages) -> Turn
  |          - API call with retry loop (max 2 retries)
  |          - validate_discussion_response(text) -> ParsedResponse
  |          - strip_structured_block(text) -> prose content
  |       4. session.turns.push(turn)
  |
  |-- vote phase:
  |     for each agent in rotation:
  |       1. vote_prompt(question) -> system context
  |       2. _build_full_transcript(session) -> user message
  |       3. agent.cast_vote(system, messages) -> Vote
  |          - API call with retry loop (max 2 retries)
  |          - validate_vote_response(text) -> ParsedVote
  |       4. session.votes.push(vote)
  |
  v
format_decision_record(session) -> stdout
save_report(session) -> logs/*.md
```

## Type Mappings

### Core types (`types.py` -> `src/types.rs`)

```
ParsedResponse   -> struct ParsedResponse { position: String, reasoning: Vec<String>, concerns: Vec<String>, updated_by: Vec<String> }
ParsedVote       -> struct ParsedVote { vote: VoteChoice, reason: String }
Turn             -> struct Turn { agent: String, round: u32, content: String, parsed: ParsedResponse }
Vote             -> struct Vote { agent: String, vote: VoteChoice, reason: String }
Session          -> struct Session { question: String, turns: Vec<Turn>, votes: Vec<Vote> }
Literal["yay","nay"] -> enum VoteChoice { Yay, Nay }
Session.outcome  -> impl Session { fn outcome(&self) -> Outcome }
Literal["approved","rejected"] -> enum Outcome { Approved, Rejected }
```

### Config (`config.py` -> `src/config.rs`)

```
CouncilConfig    -> struct CouncilConfig { rotation: Vec<String>, rounds: u32, model: String, tools: HashMap<String, Vec<String>>, backend: Backend }
"api"|"agent-sdk" -> enum Backend { Api, AgentSdk }
```

### Agent interface (`agent_base.py` -> `src/agent.rs`)

```
AgentBackend Protocol -> trait AgentBackend { fn respond(...) -> Result<Turn>; fn cast_vote(...) -> Result<Vote>; }
```

## Dependency Mappings

| Python | Rust crate | Notes |
|---|---|---|
| `anthropic` | `reqwest` + raw API | No official Rust SDK; use HTTP client |
| `python-dotenv` | `dotenvy` | |
| `argparse` | `clap` | Derive API recommended |
| `json` | `serde_json` | |
| `re` | `regex` | |
| `pathlib` | `std::path::PathBuf` | |
| `dataclasses` | `#[derive(...)]` structs | |
| `typing.Protocol` | `trait` | |
| `pytest` | `#[cfg(test)]` | |

## Key Design Notes

1. **Retry logic** lives inside each backend (not in orchestrator). Retry prompt
   constants are shared via `agent_base` / the trait module.

2. **Schema validation** uses regex to extract delimited blocks, then JSON parsing.
   In Rust: `regex` crate + `serde_json`.

3. **Prompt composition** is string concatenation of markdown files loaded at startup.
   In Rust: `include_str!()` or runtime file reads.

4. **Error handling**: Python uses `ValueError`/`FileNotFoundError`. In Rust: define
   a `CouncilError` enum implementing `std::error::Error`.

5. **The Agent SDK backend** may not have Rust bindings. Evaluate whether to port it
   or defer. The API backend covers the primary use case.
