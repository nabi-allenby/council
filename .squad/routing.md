# Work Routing

How to decide who handles what.

## Routing Table

| Work Type | Route To | Examples |
|-----------|----------|----------|
| Architecture & cross-crate design | Kai | Protobuf schema changes, crate boundaries, API design |
| Rust implementation | Nova | Feature work, bug fixes, gRPC services, CLI commands |
| Protobuf & gRPC contracts | Nova + Kai | Service definitions, message types, backward compat |
| Testing | Jax | Unit tests, integration tests, edge cases, CI |
| Code review | Kai | Review PRs, check quality, suggest improvements |
| AI integration | Nova | Anthropic API calls, agent backends, model selection |
| Scope & priorities | Kai | What to build next, trade-offs, decisions |
| Session logging | Scribe | Automatic — never needs routing |

## Issue Routing

| Label | Action | Who |
|-------|--------|-----|
| `squad` | Triage: analyze issue, assign `squad:{member}` label | Kai |
| `squad:kai` | Architecture decisions, cross-crate work | Kai |
| `squad:nova` | Implementation work across crates | Nova |
| `squad:jax` | Test coverage, test infrastructure | Jax |

### How Issue Assignment Works

1. When a GitHub issue gets the `squad` label, **Kai** triages it — analyzing content, assigning the right `squad:{member}` label, and commenting with triage notes.
2. When a `squad:{member}` label is applied, that member picks up the issue in their next session.
3. Members can reassign by removing their label and adding another member's label.
4. The `squad` label is the "inbox" — untriaged issues waiting for Kai's review.

## Rules

1. **Eager by default** — spawn all agents who could usefully start work, including anticipatory downstream work.
2. **Scribe always runs** after substantial work, always as `mode: "background"`. Never blocks.
3. **Quick facts -> coordinator answers directly.** Don't spawn an agent for "what port does the server run on?"
4. **When two agents could handle it**, pick the one whose domain is the primary concern.
5. **"Team, ..." -> fan-out.** Spawn all relevant agents in parallel as `mode: "background"`.
6. **Anticipate downstream work.** If a feature is being built, spawn Jax to write test cases from requirements simultaneously.
7. **Issue-labeled work** — when a `squad:{member}` label is applied to an issue, route to that member. Kai handles all `squad` (base label) triage.
