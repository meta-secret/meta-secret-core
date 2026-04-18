# AI Automation — meta-secret-core

🤖 Unified AI agents, commands, and rules for **Claude Code**, **Cursor**, and **OpenAI Codex CLI**.

---

## 🎯 Quick Start

### In Claude Code

```bash
/help                    # List all commands
/only-planner <task>     # Create implementation plan
```

### In Cursor

- Agents auto-discovered from `agents/`
- Rules auto-discovered from `rules/`
- Use in custom rules or inline chat

### OpenAI Codex CLI

```bash
codex --agent code-reviewer
codex --command only-implementer
codex --rule code-style
```

---

## 📂 What's Here

| Folder | Purpose |
|--------|---------|
| **agents/** | AI personas (planner, implementer, reviewer, etc.) |
| **commands/** | Slash commands for Claude Code and Codex CLI |
| **skills/** | Reusable workflows (planning, debugging, release) |
| **rules/** | Coding standards and principles for Cursor & Codex |

---

## 🔗 How It Works

All three IDEs use **symlinks** pointing to `.ai/`:

```
.claude/agents ──┐
.cursor/agents ──├──→ .ai/agents  (single source)
.codex/agents ──┘

.claude/commands ──┐
.codex/commands ──├──→ .ai/commands
```

**Edit once, works everywhere.** ✅

---

## 📖 Full Documentation

See [ARCHITECTURE.md](./ARCHITECTURE.md) for:
- Complete folder structure
- IDE integration details
- How to add new agents/commands/rules
- Symlink verification

---

## 🚀 Common Tasks

### Run a planning session
```bash
/only-planner "add encryption to storage module"
```

### Review code changes
```bash
/only-reviewer   # Then upload diff
```

### Write tests
```bash
/only-test-author  # Generate test cases
```

### Debug a failing test
```bash
/only-debug-rca "test failure logs here"
```

### Prepare release
```bash
/only-release-notes   # Draft notes
/only-release-manager # Create branch & PR
```

---

## 📚 Resources

- **Commands catalog:** `commands/README.md`
- **Agents guide:** Each agent has `.md` file in `agents/`
- **Skills reference:** Check `skills/*/SKILL.md`
- **Coding rules:** `rules/RULES.md`

---

✅ **IDE Support:** Claude Code • Cursor • OpenAI Codex CLI  
🔄 **Status:** All symlinks verified  
📅 **Last sync:** 2026-04-18
