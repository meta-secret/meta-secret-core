# AI workflow (agents, phases, approvals)

This document describes the **multi-phase delivery pipeline** for **meta-secret-core** with **human approval** after each phase, and how to invoke **individual subagents** without the full chain.

**Canonical project rules:** [CLAUDE.md](CLAUDE.md), [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md), [ARCHITECTURE.md](ARCHITECTURE.md), [SECURITY.md](SECURITY.md), [CODE_STYLE.md](CODE_STYLE.md).

## Subagent definitions

| Role | Subagent name (invoke by name) | Purpose |
|------|----------------------------------|---------|
| GitHub issue fetch + Summary | `workflow-from-issue` command (invokes `github-issue-coordinator` when run with MetaSecret context) | Load issue via `gh`, summarize, list next steps |
| Plan only | `feature-planner` | Structured plan, no code |
| Implement | `code-implementer` | Rust changes per approved plan |
| Tests | `test-author` | Add/update tests |
| Run tests | `test-verifier` | `cargo` / bake test report |
| Debug / RCA | `debug-rca` | Root cause, no repo writes by default |
| Review | `code-reviewer` | Architecture/style findings |
| Release notes | `release-notes` | MR/changelog text, no git |
| Release / MR | `release-manager` | Branch from `main`, commit/push only after explicit user ok |
| Pattern → skill/command (optional) | `workflow-pattern-capture` | 0–2 durable suggestions or “no change”; not every MR |

Files: [`.cursor/agents/`](.cursor/agents/) and [`.claude/agents/`](.claude/agents/) (same prompts; frontmatter may differ).

## Two entry points (same pipeline after planning)

| Entry | First phase | Artifact before your approval |
|-------|-------------|--------------------------------|
| **GitHub issue** (number or URL) | `/workflow-from-issue <n>` → **Summary** approval → **`/only-planner`** or `feature-planner` (your next step) | Issue summary (title, description, acceptance) |
| **Manual prompt** (feature or bug description) | Skip coordinator; go to `feature-planner` with a **task brief** (use skill `workflow-manual-task-brief`) | Task brief + plan |

After the first approved plan, the pipeline is identical.

## Phased pipeline (default order)

1. **Context** — issue path: coordinator output; manual path: your task brief + planner.
2. **Plan** — `feature-planner` → you approve.
3. **Implement** — `code-implementer` → you approve diff.
4. **Tests** — `test-author` → you approve test diff.
5. **Verify** — `test-verifier` → you review pass/fail stats.

**If tests fail or build fails:** `debug-rca` → approve → back to **Plan** (`feature-planner`) → **Implement** → **Tests** → **Verify** (loop until green).

**Optional (after `cargo` / Docker build failures or unclear workspace errors):** narrow with **systematic-debugging** skill (via `debug-rca`) and the smallest reproducing command from [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md).

**If verify is green:** if build still fails in another target (e.g. WASM, server image), treat like failure branch (`debug-rca` → plan → …).

**If green:** `code-reviewer` → if must-fix items → back to **Plan** → **Implement** (and tests as needed).

**If review ok:** `release-notes` (draft MR body) → approve → `release-manager` (branch from `main`, **commit and push only after explicit “ok”**, MR via `glab` when available).

**Optional — pattern capture (not every MR):** when a **trigger** applies—large feature, **new** error class, **same** review correction **three or more** times, or **toolchain/stack** change—run **`workflow-pattern-capture`** (skill **`workflow-pattern-capture`**) after `code-reviewer` or after `release-notes`. Output is **0–2** concrete proposals (skill, command, Cursor rule, or justified Claude hook) **or** **No changes recommended**. Skip for trivial fixes.

## Approval rule

After **every** phase, require a clear **artifact** (summary, plan, diff, test report, review notes) and **your explicit approval** before starting the next phase. Do not skip approval for “small” changes unless you explicitly choose to.

## Standalone invocation (no chain)

You can invoke **any** subagent alone with a direct prompt (logs, files, partial context):

- **Claude Code:** delegate to the named subagent or use slash commands under [`.claude/commands/`](.claude/commands/) (`/only-planner`, `/only-implementer`, etc.).
- **Cursor:** in Agent chat, use `/subagent-name` or natural language (“use the feature-planner subagent to …”). See [`.cursor/commands/README.md`](.cursor/commands/README.md) for parity.

## Skills (templates)

| Skill folder | Use |
|--------------|-----|
| `workflow-issue-handoff` | Build **Summary** after `gh issue view` (or `glab issue view`) |
| `workflow-manual-task-brief` | Structure a manual task before planning |
| `workflow-plan-output` | Plan shape; aligns with `write-implementation-plan` |
| `workflow-mr-body` | MR title/body checklist |
| `systematic-debugging` | Loaded by `debug-rca` (Claude) or read explicitly |
| `workflow-pattern-capture` | Optional: repeating patterns → skill/command/rule/hook; cap 0–2 |
| `architecture-guardian` | Layer/boundary checks for agents |
| `write-implementation-plan` | Deeper plan template |

Paths: [`.claude/skills/`](.claude/skills/).

## Tool limits

- **Cursor:** subagents do not nest; run phases sequentially.
- **Claude Code:** subagents do not spawn subagents; chain from the **main** session or run one phase per command.

## Cross-repo note

If the change **exports or changes FFI/UniFFI**, call out **meta-secret-compose** impact in the plan and release notes.
