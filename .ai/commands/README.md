# meta-secret-core — slash commands (help catalog)

Use this file when the **workspace root is this repository** (`meta-secret-core/`). Slash names here are **`/only-*`**, **`/workflow-from-issue`**, **`/workflow-from-prompt`** (no `core-` prefix).

Canonical behavior and arguments live in each `*.md` file in this folder. See [WORKFLOW.md](../WORKFLOW.md) for the full pipeline.

MetaSecret parent workspace: if your editor root is the **MetaSecret** folder, use prefixed wrappers (`/core-only-*`) from root `.ai/commands/`.

## Help

| Slash command | Action |
|---------------|--------|
| `/help` | Assistant reads this README and prints a formatted list (read-only). |

## Workflow Entry

| Slash command | Description |
|---------------|-------------|
| `/workflow-from-issue` | Start from GitHub issue or URL, produce Summary/understanding artifact, then continue with planner. |
| `/workflow-from-prompt` | Start from free-text task, produce task brief, then continue with planner. |

## Single-phase (`only-*`) commands

| Slash command | Description |
|---------------|-------------|
| `/only-planner` | Plan-only stage with structured implementation plan. |
| `/only-implementer` | Implement approved plan with minimal diffs. |
| `/only-reviewer` | Read-only review for architecture/style/security/correctness. |
| `/only-test-author` | Add or update automated tests. |
| `/only-test-verifier` | Run verification tests and report pass/fail. |
| `/only-debug-rca` | Root-cause analysis from logs/failures. |
| `/only-release-notes` | Draft PR/release notes; **`gh pr edit`** when updating title/body (see workflow-mr-body skill). |
| `/only-release-manager` | Branch, commit, push, open/update PR via `gh pr create` / **`gh pr edit`** (with explicit approvals). |
| `/only-issue-coordinator` | Fetch GitHub issue and format Summary for planner handoff. |
| `/only-workflow-pattern-capture` | Propose workflow improvements from repeated patterns. |

## Agents

Definitions: [../agents/](../agents/)

Last updated: 2026-04-22
