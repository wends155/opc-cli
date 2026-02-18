---
description: Intake, investigate, and report an issue before planning (Pre-Think Phase)
---

# Issue Workflow

This workflow defines the standard process for receiving, investigating, and
documenting an issue **before** any planning or implementation begins.
It is the entry point of the TARS cycle — the step that comes before `/plan-making`.

> [!IMPORTANT]
> This workflow is **read-only** — no code edits, no plans, no implementation.
> The only output is a structured **Issue Report** artifact.

## Trigger

User invokes: `/issue <description>`

## Prerequisites

- Read `architecture.md` (if present) for project structure, components, and toolchain.
- Read `context.md` (if present) for historical decisions and known issues.
- Confirm you are operating as the **Architect** role.

## Steps

### 1. Parse & Classify

Extract the following from the user's description:

| Field         | Action                                                       |
|---------------|--------------------------------------------------------------|
| **Type**      | Classify: `bug`, `feature`, `chore`, `docs`, or `question`  |
| **Component** | Identify the affected area (e.g., TUI, API, CLI, Database)  |
| **Severity**  | Estimate: `critical`, `high`, `medium`, `low`                |
| **Summary**   | One-line restatement of the issue                            |

If the description is too vague to classify, **ask clarifying questions immediately**
before proceeding to Step 2.

### 2. Load Context

Gather background information:

- **`architecture.md`**: Identify relevant modules, patterns, and frameworks.
- **`context.md`**: Check for prior decisions, known bugs, or related history.
- **`git log -n 20`**: Review recent commits for changes in the affected area.
- **Existing issues/TODOs**: Search for related `TODO`, `FIXME`, `HACK` comments in the codebase.

### 3. Investigate

Search the codebase to understand the problem area:

- **Identify suspect files**: `grep` / `ripgrep` for keywords related to the issue.
- **Read relevant code**: Outline the affected functions/modules.
- **Map dependencies**: What calls into or depends on the affected code?
- **Look for obvious causes**: Missing error handling, logic errors, race conditions, etc.
- **Check tests**: Are there existing tests covering this area? Are they passing?

> [!TIP]
> Keep investigation focused. The goal is to understand the problem well enough to
> write a clear report — not to find the exact fix (that's for `/plan-making`).

### 4. Produce Issue Report

Create a structured report with the following format:

```markdown
## 🐛 Issue Report

| Field          | Value                        |
|----------------|------------------------------|
| **Type**       | bug / feature / chore / docs |
| **Component**  | [affected area]              |
| **Severity**   | critical / high / med / low  |
| **Filed**      | [date]                       |

### Description
[Clear restatement of the issue in the user's own words]

### Investigation Findings
- **Suspect files:** [list of files with links]
- **Root cause hypothesis:** [best guess based on investigation]
- **Related history:** [anything from context.md or git log]
- **Recent changes:** [relevant commits, if any]
- **Test coverage:** [existing tests in this area, pass/fail status]

### Open Questions
- [Any ambiguities or unknowns that need user clarification]

### Recommended Severity
[Confirm or adjust the initial severity estimate with reasoning]
```

### 5. Pause for Refinement

End the report with:

> 🛑 **Issue Analysis Complete.**
> Please review the findings above. You can:
> - **Clarify** or **refine** the issue description
> - **Adjust** severity or component classification
> - **Add** additional context or constraints
>
> When satisfied, reply with **"Plan"** to proceed to `/plan-making`.

**Do NOT proceed to planning until the user explicitly approves the issue report.**

## Rules

1. **No code edits** — this is an investigation-only workflow.
2. **No planning** — do not propose solutions or implementation steps.
3. **Always pause** — the user must explicitly say "Plan" to move forward.
4. **Ask early** — if the issue is ambiguous, ask questions in Step 1, not Step 4.
5. **Stay focused** — investigate just enough to produce a clear report; avoid rabbit holes.
