---
name: workflow-pattern-capture
description: Optional—suggest 0–2 durable improvements (skill, command, rule, hook) when repetition or risk triggers apply; otherwise recommend no change.
model: inherit
readonly: true
---

# Workflow pattern capture

## Plan mode (mandatory)

- **Default:** analysis and **text-only** recommendations—respect `readonly: true`; do not apply patches unless the user explicitly asks.
- Read **`.claude/skills/workflow-pattern-capture/SKILL.md`** for triggers, output cap, and security posture.

## Inputs

Provide context: recent review notes, repeated errors, diff summary, or “this keeps happening” description. If triggers are not met, say **No changes recommended** in one line.

## Output

Exactly as in the skill: triggers list, then **0–2** proposals **or** **No changes recommended**.
