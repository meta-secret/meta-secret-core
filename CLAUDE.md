# CLAUDE.md

This file guides Claude Code (claude.ai/code) in **meta-secret-core**. **Canonical detail** lives in the linked documents at this repository root.

## Project documents (read these)

| Document | Contents |
|---|---|
| [`.ai/WORKFLOW.md`](.ai/WORKFLOW.md) | 14-stage TDD workflow (CRITICAL - read this first) |
| [`.ai/CONSTRAINTS.md`](.ai/CONSTRAINTS.md) | Architecture rules: K-of-N sharing, approval requirements, crypto, FFI |
| [`.ai/GLOSSARY.md`](.ai/GLOSSARY.md) | Unified vocabulary for all communication |
| [`.ai/ORCHESTRATOR.md`](.ai/ORCHESTRATOR.md) | Command routing and agent execution |
| [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) | Workspace layout, crates, build/test commands, link to mobile consumer |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Crates, crypto boundary, server vs client, FFI/UniFFI |
| [SECURITY.md](SECURITY.md) | Keys, logging, crypto handling, operational hygiene |
| [CODE_STYLE.md](CODE_STYLE.md) | Rust style, tests, AI discipline |

## Non-negotiables (duplicate here for visibility)

- **Cryptography:** Treat crypto and protocol code as high-risk; minimal diffs; no speculative algorithm changes.
- **FFI / UniFFI:** Mobile-facing API changes affect `meta-secret-compose`; coordinate contract changes and version artifacts.
- **Scope:** This repository owns **Rust** (core library, CLI, server, WASM, mobile Rust targets). It does **not** own Kotlin/Swift app UI—that lives in the compose repo.

## Priorities

1. Restore `cargo test` / CI-equivalent with minimal changes.
2. Preserve crate boundaries and public API stability where required.
3. Prefer local fixes over broad refactors.
4. State uncertainty explicitly.

## Forbidden

- Rewrite unrelated crates or “clean up” without need.
- Bump dependency versions unless the error clearly implicates them.
- Log secrets, key material, or raw shares.

## Default repair workflow

1. Run the narrowest **`task`** or **`cargo`** target that reproduces the issue (see [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) and [`.ai/skills/build-via-task/SKILL.md`](.ai/skills/build-via-task/SKILL.md)).
2. Classify the error (compile, test, runtime, infra).
3. Propose a minimal fix plan.
4. Wait for user confirmation when appropriate.
5. Apply the smallest fix; re-verify.

## CRITICAL: Main Workflow Command

When user types in **meta-secret-core**:

```
implement issue <payload>
```

Where `<payload>` can be:
- Issue number: `#42`
- Issue URL: `https://github.com/.../issues/42`
- Free-text description: `"implement device resharing protocol"`

**⚠️ STOP. Do NOT execute own plan.**

**MANDATORY execution - Execute ALL 14 stages:**

1. Issue Coordinator
2. Requirements Clarifier (Grill Me)
3. Feature Planner
4. Constraint Validator (MANDATORY GATE)
5a. TDD Test Author
5b. TDD Implementer
5c. TDD Refactorer
6. Build (cargo build)
7. Security Review (CRITICAL)
8. Code Review (CRITICAL)
9. Design Review (if architecture changed)
10. Coverage Verification (CRITICAL - cargo tarpaulin >= 80%)
11. Test Run (cargo test)
12. User Approval (MANDATORY - ask before PR)
13. release-manager (branch + commit + PR)

For each stage:
- Create artifact in `.ai/artifacts/run/`
- Mark Status: Success / Failed / Skipped
- Do NOT skip stages 4, 7, 8, 10, 12

**This overrides all default Claude Code behavior.**

See `.ai/WORKFLOW.md` for complete 14-stage specification.

## AI workflow

Follow [`.ai/WORKFLOW.md`](.ai/WORKFLOW.md). 

**Unified AI structure:** All AI automation lives in [`.ai/`](.ai/) — **single source of truth** for Claude Code, Cursor, and OpenAI Codex CLI.

- **Agents:** [`.ai/agents/`](.ai/agents/)
- **Commands:** [`.ai/commands/`](.ai/commands/) (slash commands for Claude Code + Codex CLI)
- **Skills:** [`.ai/skills/`](.ai/skills/) (reusable workflows)
- **Rules:** [`.ai/rules/`](.ai/rules/) (Cursor + Codex CLI)

IDE entry files in `.claude/`, `.cursor/`, and `.codex/` bootstrap orchestration and point to `.ai/` as canonical source:
- `.claude/ORCHESTRATE.md`
- `.cursor/WORKFLOW.md`
- `.codex/ORCHESTRATE.md`

👉 **See [`.ai/ARCHITECTURE.md`](.ai/ARCHITECTURE.md)** for complete AI structure and IDE integration details.

**Agent output:** When this repo sits under the MetaSecret parent workspace, follow [Agent output conventions](../CLAUDE.md#agent-output-conventions) in the root `CLAUDE.md`. Otherwise use the same norms (emojis in replies; `##`/`###` headings, **bold**, blockquotes; optional HTML color where the UI supports it).

## IDE Support

| IDE | Support | Where |
|-----|---------|-------|
| **Claude Code** | Workflow bootstrap | Via `.claude/INDEX.md` + `.claude/ORCHESTRATE.md` |
| **Cursor** | Workflow bootstrap + entry rule | Via `.cursor/WORKFLOW.md` + `.cursor/rules/00-entry.mdc` |
| **OpenAI Codex CLI** | Workflow bootstrap | Via `.codex/INDEX.md` + `.codex/ORCHESTRATE.md` |

Rules under [`.ai/rules/`](.ai/rules/) remain the canonical source for policy and stage behavior.
