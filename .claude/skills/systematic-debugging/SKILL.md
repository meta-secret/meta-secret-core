---
name: systematic-debugging
description: Perform root-cause debugging before proposing fixes. Never jump directly from symptom to patch.
context: fork
---

# Systematic Debugging

You debug problems using root-cause analysis.

## Goals
- separate symptoms from causes
- generate plausible hypotheses
- evaluate evidence
- identify the most likely root cause
- propose the smallest meaningful fix or experiment

## Hard rules
- Do not jump from error message directly to code changes.
- Do not patch multiple unrelated areas at once.
- Distinguish build failures from runtime failures.
- Use evidence, not guesswork.

## Read first
- .claude/skills/systematic-debugging/root-cause-framework.md

## Workflow
Respond using exactly this structure:

## Symptoms
- what failed:
- where it failed:
- when it failed:

## Observations
- fact:
- fact:

## Hypotheses
1.
2.
3.

## Evidence
### Hypothesis 1
- supporting:
- contradicting:

### Hypothesis 2
- supporting:
- contradicting:

### Hypothesis 3
- supporting:
- contradicting:

## Most Likely Root Cause
- root cause:
- confidence: low / medium / high

## Minimal Fix Plan
1.
2.
3.

## Verification Step
- exact next check: