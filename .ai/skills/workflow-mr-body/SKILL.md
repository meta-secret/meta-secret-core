---
name: workflow-mr-body
description: Draft and update PR/MR title and description from full branch scope; use before gh pr create or gh pr edit.
---

# Workflow — MR body

Use with **release-notes** (draft) and **release-manager** (create/update PR). Keeps MR text consistent with [SECURITY.md](../../../SECURITY.md) (no secrets in text).

## Read first

- [mr-body-template.md](mr-body-template.md)

## Scope rule (mandatory)

The PR title and description must reflect **the entire branch**, not only the first commit or the original PR text.

Before writing or updating copy:

1. `git log main..HEAD --oneline` — all commits on the branch
2. `git diff main...HEAD --stat` — full diff vs base
3. `gh pr view --json title,body` — current PR text (if a PR already exists)

If new commits landed after the PR was opened, **update** title and body before asking the user to merge.

## Rules

- Summarize user-visible changes and risk
- Group related work (e.g. UI, Docker/cache, CI/agents) when the branch spans multiple themes
- List test commands run or “not run (reason)” — prefer `task …` from [build-via-task](../build-via-task/SKILL.md)
- Do not paste tokens or internal-only URLs unless the user provided them for the MR
- Do not invent shipped behavior not evidenced by the diff or commit list

## Create PR

After user approves title and body:

```bash
gh pr create --title "…" --body "$(cat <<'EOF'
…
EOF
)"
```

## Update existing PR (mandatory when scope grew)

When a PR already exists and the branch has new themes or commits since the last description:

```bash
gh pr edit <number> --title "…" --body "$(cat <<'EOF'
…
EOF
)"
```

Also run when the user asks to **update title and description of the PR**, or before final merge if the body is stale.

Use PR number from `gh pr view` / branch tracking, or `gh pr list --head "$(git branch --show-current)"`.

## Title guidance

- One line; cover the main themes if the branch is multi-part (e.g. `feature A + infra B`)
- Prefer concrete scope over generic “fix stuff” or only the original feature name

## When to invoke

| Trigger | Action |
|---------|--------|
| Stage 8 / `/only-release-manager` | Draft from full branch; create or `gh pr edit` |
| User: “update PR title/description” | Re-read diff + commits; `gh pr edit` |
| Significant push after PR open | Propose updated title/body; edit after user ok |
| `/only-release-notes` | Draft only; release-manager applies via `gh pr edit` |
