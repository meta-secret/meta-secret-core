---
description: List meta-secret-core slash commands (read-only, formatted); never edit code or project files.
---

# Help — meta-secret-core AI slash commands

**Arguments:** `$ARGUMENTS` (optional filter keywords to narrow sections).

## Read-only scope (Ask-style)

Treat this command like **Ask mode**: **informational only**.

- **Do not** modify, create, delete, or refactor any source files, configs, or project assets.
- **Do not** run builds, tests, installs, formatters, or git write operations as part of this command.
- **Allowed:** read **`.claude/commands/README.md`** in this repository (same directory as this file). That file is the **single source of truth** for the catalog.

## Instructions for the assistant

1. Read **`.claude/commands/README.md`** at `meta-secret-core/.claude/commands/README.md`.

2. Reply with **Markdown**: clear **`##` / `###` headings**, **tables** for slash commands, **bold** slash names (e.g. **`/only-planner`**). Use **emoji section labels** for grouping (consistent with the MetaSecret root `CLAUDE.md` agent output conventions when this repo sits under the MetaSecret workspace).

3. Reproduce the **Description** and **Action** columns from the README; do not substitute bare “delegates to …” file paths for those columns.

4. Mention that **`/help`** in this repo reads **this** README; the MetaSecret parent workspace has a separate catalog with `/core-*` prefixes if the user uses that layout.

5. Do **not** invent commands not listed in the README.
