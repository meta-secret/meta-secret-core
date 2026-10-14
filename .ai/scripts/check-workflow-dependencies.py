#!/usr/bin/env python3
from pathlib import Path
import sys
root = Path(__file__).resolve().parents[2]
required = ['.ai/WORKFLOW.md', '.ai/agents/security-reviewer.md', '.ai/artifacts/clarification-template.md', '.ai/artifacts/design-review-report-template.md', '.ai/artifacts/coverage-report-template.md']
missing = [p for p in required if not (root / p).is_file()]
root_contract = [".ai/agents/ui-e2e-test-author.md", ".ai/agents/ui-e2e-test-runner.md", ".ai/skills/ui-e2e-testing/SKILL.md", ".ai/rules/ui-e2e-test-contract.md"]
if (root.parent / ".ai").is_dir():
    missing += ["../" + p for p in root_contract if not (root.parent / p).is_file()]
else:
    print("Root UI E2E dependencies unavailable in standalone checkout; verify before the final gate")
if missing:
    print("Missing mandatory AI dependencies: " + ", ".join(missing), file=sys.stderr)
    sys.exit(1)
print("Mandatory AI workflow dependencies exist")
