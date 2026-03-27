---
name: workflow-pattern-capture
description: Optional retrospective—suggest 0–2 repo improvements (skill, command, rule, or hook) when triggers fire; otherwise say to change nothing.
---

# Workflow — pattern capture (process improvement)

Use this **sparingly**. Goal: turn **repeating** human or model behavior into **durable** assets (skills, commands, Cursor rules, or—only when justified—Claude hooks), including **security-sensitive** automation ideas.

## When to run (triggers)

Run only if **at least one** applies:

- **Large feature** or multi-file change that will recur in similar form.
- **New class of failures** (build, tests, runtime) that may appear again.
- **Same review comment or correction** appeared **three or more times** across sessions or MRs (pattern, not one-off nit).
- **Stack or toolchain change** (Rust edition, Cargo workspace, Docker bake, FFI/UniFFI) that invalidates old habits.

If none apply, **stop** and output **No changes recommended** (one line is enough).

## Read first (as needed)

- [SECURITY.md](../../../SECURITY.md) — before suggesting anything that touches secrets, credentials, or enforcement.
- [WORKFLOW.md](../../../WORKFLOW.md) — avoid duplicating existing agents/skills.
- [Other skills in this repo](../) — prefer **extending** an existing skill over adding a new one.

## What to propose

- **Skill** — repeatable instructions or checklists (new folder under `.claude/skills/` or a short addition to an existing `SKILL.md`).
- **Slash command** — repeatable entry under [`.claude/commands/`](../../commands/) (mirror usage in [`.cursor/commands/README.md`](../../../.cursor/commands/README.md)).
- **Cursor rule** — persistent guidance under [`.cursor/rules/`](../../../.cursor/rules/) when the issue is editor/agent behavior, not `cargo` defaults.
- **Claude hook** — only when **automated session-level control** is truly needed (e.g. block writes to forbidden paths, audit log). Hooks are **not** a substitute for CI or `cargo test`; say **no** when a skill or rule is enough.
- **`## Gotchas` section** — when the model **repeatedly fails** in the same way (wrong module placement, ignored FFI boundary, incorrect Cargo feature flags): add a `## Gotchas` section to the relevant skill documenting the failure mode and correct behavior. This is the highest-ROI addition to any skill.
- **Dynamic injection** — for build/diagnostic skills (e.g. `systematic-debugging`, Docker bake helpers): consider embedding a shell command using `` !`command` `` syntax in `SKILL.md` so the model receives live output on invocation (e.g. active Rust toolchain, recent `cargo check` output). Only propose when the dynamic context materially changes how the model should act.

## Output format (strict)

1. **Triggers satisfied** — bullet list (which bullets from the section above apply), or state that none apply and end with **No changes recommended**.
2. **Proposals** — **at most two** items. For each: **type** (skill | command | rule | hook), **title**, **why** (evidence: repetition, risk class), **scope** (files or folders to add/change), **not doing** (what you explicitly reject to avoid bloat).
3. If **no** items: a single line **No changes recommended** and optional **one sentence** why (e.g. one-off fix, already covered by `WORKFLOW.md`).

Do **not** list more than two proposals. Do **not** invent hooks for convenience.

## Output language

Match the user’s language for the narrative; **identifiers and paths** stay as in the repo (English).
