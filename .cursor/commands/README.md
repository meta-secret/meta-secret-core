# Cursor commands (workflow parity)

Cursor does not use the same `/slash` command files as Claude Code. Use this table to get **equivalent behavior** in **Agent** chat.

| Intent | What to type |
|--------|----------------|
| Workflow overview | Open [WORKFLOW.md](../../WORKFLOW.md) or ask Agent to follow it |
| From GitHub issue | Ask Agent to run the **github-issue-coordinator** subagent with the issue ref, then approve before planning |
| From manual prompt | Ask Agent to follow **`only-from-prompt`** (or apply **workflow-manual-task-brief** skill, then **feature-planner**) |
| Plan only | `/feature-planner` or: “Use the **feature-planner** subagent: …” |
| Implement only | “Use the **code-implementer** subagent: …” |
| Tests only | “Use the **test-author** subagent: …” |
| Verify tests | “Use the **test-verifier** subagent: …” |
| Debug / RCA | “Use the **debug-rca** subagent: …” |
| Review | “Use the **code-reviewer** subagent: …” |
| Release notes | “Use the **release-notes** subagent: …” |
| Release / MR | “Use the **release-manager** subagent: …” |
| Pattern → skill/command (optional) | “Use the **workflow-pattern-capture** subagent with triggers from skill **workflow-pattern-capture**: …” |

Subagent definitions: [`.cursor/agents/`](../agents/). Skills: [`.claude/skills/`](../../.claude/skills/) (shared with Claude Code).

Claude Code slash commands live in [`.claude/commands/`](../../.claude/commands/) (including `/only-workflow-pattern-capture` for optional process capture).
