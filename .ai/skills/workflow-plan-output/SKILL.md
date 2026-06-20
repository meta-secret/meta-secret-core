---
name: workflow-plan-output
description: Expected shape of the planning phase output before code-implementer; aligns with write-implementation-plan.
---

# Workflow — plan output

The **feature-planner** subagent owns the planning phase. Plans must be architecture-safe and file-oriented.

## Read first

- [CLAUDE.md](../../../CLAUDE.md)
- [ARCHITECTURE.md](../../../ARCHITECTURE.md)
- [../write-implementation-plan/plan-template.md](../write-implementation-plan/plan-template.md)
- Optional deeper plan: [../write-implementation-plan/SKILL.md](../write-implementation-plan/SKILL.md)

## Output checklist

1. Goal and non-goals
2. Crates and files touched (paths under `meta-secret/`)
3. Ordered steps
4. Risks
5. Verification commands: **`task …`** from repo root for Docker/CI parity (see [`.ai/skills/build-via-task/SKILL.md`](../skills/build-via-task/SKILL.md)); narrow `cargo` from `meta-secret/` when appropriate
6. Explicit **wait for user approval** before implementation

Do not duplicate full templates here—use `write-implementation-plan/plan-template.md` for the question list.
