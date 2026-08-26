---
description: List meta-secret-core slash commands (read-only, formatted); never edit code or project files.
---

# Help — meta-secret-core AI slash commands

**Arguments:** `$ARGUMENTS` (optional filter keywords to narrow sections).

## Read-only scope (Ask-style)

Treat this command as informational only.

- Do not modify, create, delete, or refactor any source files, configs, or project assets.
- Do not run builds, tests, installs, formatters, or git write operations as part of this command.
- Allowed: read `.ai/commands/README.md` in this repository as the command catalog source.

## Instructions for the assistant

1. Read `.ai/commands/README.md` at `meta-secret-core/.ai/commands/README.md`.
2. Reply with Markdown: clear `##` / `###` headings, tables for slash commands, and bold slash names.
3. Reproduce command descriptions from the README.
4. Mention that `/help` in this repo reads this README; MetaSecret parent workspace has a separate catalog with `/core-*` wrappers.
5. Do not invent commands not listed in the README.
6. Emphasize that `/implement issue <payload>` is the PRIMARY command for all feature work.
