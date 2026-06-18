---
name: write-skill
description: >-
  Write or update project skills and rules markdown files (.ai/skills/, .ai/rules/, .cursor/rules/).
  Use when creating a new SKILL.md or rule, updating an existing one, or when the user asks to
  document a workflow, pattern, or convention as a reusable skill.
disable-model-invocation: true
---

# Writing Skills and Rules

## Non-negotiables

**Never include directory structure trees** (no ASCII trees like `foo/`, `├──`, `└──`).
Replace any structure description with a plain table or inline file references.

**No** — do not write:
```
.github/
  scripts/
    lib/
      fetch-logs.ts   ← does X
```

**Yes** — write instead:
| File | Role |
|------|------|
| `.github/scripts/lib/fetch-logs.ts` | Does X |

Or inline: "The entrypoint is `.github/scripts/cursor-fix.ts`; logs are read by `lib/fetch-logs.ts`."

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

`disable-model-invocation: true` is the default — omit only if the skill should activate automatically from ambient context.

## Content rules

- **One skill = one concern.** If it does two things, split it.
- **Assume the agent is smart.** Only write what it cannot infer from general knowledge.
- **No directory trees** (see above).
- **No verbose preamble.** Skip "This skill helps you…" — get to the instructions.
- **No time-sensitive notes** ("as of June 2026…").
- **Prefer tables over lists** when mapping names to roles, options to descriptions, etc.
- Keep SKILL.md under 150 lines. Extract reference material to a sibling file and link to it.

## Description checklist

- Third person ("Generates…", "Reviews…", not "I can…" or "Use me to…")
- Includes trigger terms (specific function names, file patterns, keywords)
- Both WHAT (capability) and WHEN (trigger scenarios)
- Under 1 024 chars

## Location

| Scope | Path |
|-------|------|
| This project | `.ai/skills/<name>/SKILL.md` |
| Personal (all projects) | `~/.cursor/skills/<name>/SKILL.md` |

Never write into `~/.cursor/skills-cursor/` — that is Cursor's internal directory.
