# Squad Team

> council — Turn-based AI deliberation system

## Coordinator

| Name | Role | Notes |
|------|------|-------|
| Squad | Coordinator | Routes work, enforces handoffs and reviewer gates. |

## Members

| Name | Role | Charter | Status |
|------|------|---------|--------|
| Kai | Lead / Architect | [charter](agents/kai/charter.md) | Active |
| Nova | Backend Developer | [charter](agents/nova/charter.md) | Active |
| Jax | Test Engineer | [charter](agents/jax/charter.md) | Active |
| Scribe | Scribe | [charter](agents/scribe/charter.md) | Active |
| Ralph | Ralph | [charter](agents/ralph/charter.md) | Active |

## Project Context

- **Project:** council
- **Language:** Rust (2021 edition)
- **Stack:** gRPC/Tonic, Protobuf/Prost, Clap, Tokio, Anthropic Claude API
- **Structure:** Cargo workspace with `council-cli`, `council-daemon`, `council-proto`
- **Created:** 2026-03-24
