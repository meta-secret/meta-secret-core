---
description: Start delivery from a GitHub issue — fetch with gh, format Summary (emoji + Markdown), stop for approval before planning.
---

# Workflow from issue

Arguments: issue reference (number or URL). Example: `/workflow-from-issue 81`

1. Run the **github-issue-coordinator** subagent with `ISSUE=$ARGUMENTS`.
   - If running directly in `meta-secret-core`, call `gh issue view <n> --repo meta-secret/meta-secret-core` and format the result with skill `workflow-issue-handoff`.
2. Apply skill **workflow-issue-handoff** (`.claude/skills/workflow-issue-handoff/`) to format the **Summary**.
3. **Stop.** Wait for explicit user approval of the **Summary**.
4. Next: `/only-planner` with the approved **Summary** text. If the workspace root is **MetaSecret**, use **`/core-only-planner`** instead.

Read [WORKFLOW.md](../WORKFLOW.md) for the full pipeline.

## Presentation (required for the user-visible reply)

When you output the **Summary** (after steps 1–2, before asking for approval):

1. **Formatting:** Use clear **Markdown** — headings (`##` / `###`), **bold** for issue number, title, and key fields; bullet lists for labels, acceptance, or risks. Add **emoji section labels** for quick scanning.

2. **Next steps block:** Immediately after the Summary, append one section like `## Next steps — pick a command` with follow-up slash commands and one-line descriptions.

3. Do **not** run the next phase automatically — the user chooses from the list (or approves and names the command).
