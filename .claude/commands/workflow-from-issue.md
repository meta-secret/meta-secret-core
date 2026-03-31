---
description: Start delivery from a GitHub issue — fetch with gh, format Summary (emoji + Markdown), stop for approval before planning.
---

# Workflow from issue

Arguments: issue reference (number or URL). Example: `/workflow-from-issue 81`

1. Run the **github-issue-coordinator** subagent (MetaSecret-level: `.claude/agents/github-issue-coordinator.md`) with `TARGET_SUBDIR=meta-secret-core` and `ISSUE=$ARGUMENTS`.
   - If running from the **meta-secret-core** workspace directly (not MetaSecret root), call `gh issue view <n> --repo meta-secret/meta-secret-core` and format the result with skill `workflow-issue-handoff`.
2. Apply skill **workflow-issue-handoff** (`.claude/skills/workflow-issue-handoff/`) to format the **Summary**.
3. **Stop.** Wait for explicit user approval of the **Summary**.
4. Next: `/only-planner` with the approved **Summary** text (or delegate **feature-planner** with that context). If the workspace root is **MetaSecret**, use **`/core-only-planner`** instead of `/only-planner`.

Read [WORKFLOW.md](../WORKFLOW.md) for the full pipeline.

> **Note:** Issues for this repo are on **GitHub** (`gh`). The `gitlab-issue-coordinator` agent exists for GitLab-hosted projects only.

## Presentation (required for the user-visible reply)

When you output the **Summary** (after steps 1–2, before asking for approval):

1. **Formatting:** Use clear **Markdown** — headings (`##` / `###`), **bold** for issue number, title, and key fields; bullet lists for labels, acceptance, or risks. Add **emoji section labels** for quick scanning (examples: ticket for metadata, memo for scope, checkmark for acceptance, warning for risks — pick a consistent set for that reply).

2. **Next steps block:** Immediately **after** the Summary, append **one** section with a title like `## Next steps — pick a command` (include a leading emoji, e.g. gear). Include a **table** or compact **bullet list** of follow-up slash commands with **one-line** descriptions so the user can choose the next phase without opening the README.

   - If workspace root is **MetaSecret** (parent of `meta-secret-core/`), list **`/core-only-*`** commands only (see table below).
   - If workspace root is **meta-secret-core** only, list **`/only-*`** with the same meanings (no `core-` prefix).

   | Slash (MetaSecret root) | Slash (this repo root only) | What it does |
   |-------------------------|------------------------------|--------------|
   | `/core-only-planner` | `/only-planner` | Plan only — structured plan, no code |
   | `/core-only-implementer` | `/only-implementer` | Implement approved plan |
   | `/core-only-reviewer` | `/only-reviewer` | Code / architecture review |
   | `/core-only-test-author` | `/only-test-author` | Add or update tests |
   | `/core-only-test-verifier` | `/only-test-verifier` | Run tests and report |
   | `/core-only-debug-rca` | `/only-debug-rca` | Debug / root-cause analysis |
   | `/core-only-release-notes` | `/only-release-notes` | Draft release / MR notes |
   | `/core-only-release-manager` | `/only-release-manager` | Branch, commit, push (after your ok) |
   | `/core-only-issue-coordinator` | `/only-issue-coordinator` | GitLab issue coordination (`glab`) when relevant |
   | `/core-only-workflow-pattern-capture` | `/only-workflow-pattern-capture` | Capture workflow patterns into skills/commands |

   End the block with one line: typical next step after Summary approval is **`/core-only-planner`** (MetaSecret) or **`/only-planner`** (repo root).

3. Do **not** run the next phase automatically — the user chooses from the list (or approves and names the command).
