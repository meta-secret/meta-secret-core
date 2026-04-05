# meta-secret-core — slash commands (help catalog)

Use this file when the **workspace root is this repository** (`meta-secret-core/`). Slash names here are **`/only-*`**, **`/only-issue-coordinator`**, **`/only-from-prompt`** (no `core-` prefix).

**Canonical** behavior and arguments live in each `*.md` file in this folder. See [WORKFLOW.md](../../WORKFLOW.md) for the full pipeline.

**MetaSecret parent workspace:** If your editor root is the **MetaSecret** folder that contains `meta-secret-core/` and `meta-secret-compose/`, use the prefixed wrappers instead (`/core-only-issue-coordinator`, `/core-only-planner`, …). Those are documented in the MetaSecret workspace `.claude/commands/README.md` (not in this repo).

## Help

| Slash command | Action |
|---------------|--------|
| `/help` | Assistant reads **this README** and prints a formatted list (Markdown); **read-only** — no code or repo changes ([details](help.md)). |

## GitHub issue (Summary; then continue with `only-*`)

Requires: [GitHub CLI](https://cli.github.com/) (`gh`), authenticated (`gh auth login`).

Full behavior: [only-issue-coordinator.md](only-issue-coordinator.md). Agent: [`.claude/agents/github-issue-coordinator.md`](../../.claude/agents/github-issue-coordinator.md).

| Slash command | Description |
|---------------|-------------|
| `/only-issue-coordinator` | `gh` fetch + structured issue **Summary** (Markdown, emoji sections); stop for user approval before planning; typical next step **`/only-planner`**. |

Arguments: issue **number** or **URL**.

## Manual task (no issue)

Full behavior: [only-from-prompt.md](only-from-prompt.md).

| Slash command | Description |
|---------------|-------------|
| `/only-from-prompt` | Manual task brief (skill **workflow-manual-task-brief**), approval, then **feature-planner**; continues per [WORKFLOW.md](../../WORKFLOW.md). |

## Single-phase (`only-*`) commands

Brief purpose below. Full behavior: `only-*.md` in this folder.

| Slash command | Description |
|---------------|-------------|
| `/only-planner` | Plan-mode: produce a formatted implementation plan from context; stop for approval before coding. |
| `/only-implementer` | Agent mode: implement an already-approved plan (or a narrow explicit instruction). |
| `/only-reviewer` | Plan-mode, read-only: review diffs for architecture, style, and risks; severity-grouped findings. |
| `/only-test-author` | Agent mode: add or update automated tests (not full-suite runs — use test-verifier). |
| `/only-test-verifier` | Agent mode: run the default test bundle (Cargo + web-cli npm by default) and report pass/fail. |
| `/only-debug-rca` | Plan-mode RCA from logs or failures; diagnosis first, no repo writes unless scope expands. |
| `/only-release-notes` | Draft release notes / MR copy in chat; optional file updates only if the user asks. |
| `/only-release-manager` | Agent mode: branch, commit, push, open PR — pauses for explicit user approval before git writes. |
| `/only-issue-coordinator` | Plan mode: fetch a GitHub issue (`gh`) and format a Summary for the planner handoff. |
| `/only-workflow-pattern-capture` | Plan mode: propose 0–2 workflow improvements from repeated review or process patterns. |

## Agents

Definitions: [`.claude/agents/`](../../.claude/agents/).

## Cursor

Cursor does not load these as native `/` commands. Use the same intents in Agent chat; see [`.cursor/commands/README.md`](../../.cursor/commands/README.md).
