---
name: meta-skill
description: >-
  Write or update project skills and rules markdown files (.ai/skills/, .ai/rules/, .cursor/rules/).
  Use when creating a new SKILL.md or rule, updating an existing one, or when the user asks to
  document a workflow, pattern, or convention as a reusable skill.
disable-model-invocation: true
---

# Writing Skills and Rules

## Non-negotiables

**No directory structure trees** (no ASCII trees with `├──`, `└──`, `  scripts/`, etc.).

**No tables.** Use lists, nested lists, or definition-style text instead.

**No file listings.** Don't enumerate every file in the codebase. Explain what the thing does at a general level; let the reader find the files themselves.

**No** — do not write:
```
| File | Role |
|------|------|
| fetch-logs.ts | Does X |
```

**Yes** — write instead: "Failure logs are fetched and trimmed before being passed to the agent."

## Structure

Every skill needs YAML frontmatter and a markdown body:

```markdown
---
name: your-skill-name
description: Third-person, specific. Say WHAT it does and WHEN to use it.
disable-model-invocation: true
---

# Skill Name

Concise body.
```

`disable-model-invocation: true` is the default — omit only if the skill should activate from ambient context.

## Content rules

- **One skill = one concern.** If it does two things, split it.
- **Assume the agent is smart.** Only write what it cannot infer from general knowledge.
- **No directory trees.**
- **No tables.** Lists, nested lists, or prose.
- **No verbose preamble.** Skip "This skill helps you…" — get to the instructions.
- **No time-sensitive notes** ("as of June 2026…").
- Keep SKILL.md under 150 lines. Extract reference material to a sibling file and link to it.

## Description checklist

- Third person ("Generates…", "Reviews…", not "I can…")
- Includes trigger terms (specific function names, file patterns, keywords)
- Both WHAT (capability) and WHEN (trigger scenarios)
- Under 1 024 chars

## Location

- This project: `.ai/skills/<name>/SKILL.md`
- Personal (all projects): `~/.cursor/skills/<name>/SKILL.md`

Never write into `~/.cursor/skills-cursor/` — that is Cursor's internal directory.
