---
description: Research, evaluate feasibility, and report on a new feature request (Pre-Think Phase)
---

# Feature Workflow

This workflow defines the standard process for receiving a feature request,
researching its feasibility, evaluating alternatives, and producing a structured
report **before** any planning or implementation begins.
Like `/issue`, this is a pre-planning step that feeds into `/plan-making`.

> [!IMPORTANT]
> This workflow is **read-only** — no code edits, no plans, no implementation.
> The only output is a structured **Feature Research Report** artifact.

## Trigger

User invokes: `/feature <description>`

## Prerequisites

- Read `architecture.md` (if present) for project structure, patterns, and constraints.
- Read `context.md` (if present) for historical decisions and prior feature work.
- Confirm you are operating as the **Architect** role.

## Steps

### 1. Parse & Understand

Extract the following from the user's description:

| Field            | Action                                                              |
|------------------|---------------------------------------------------------------------|
| **Feature Name** | Short, descriptive name for the feature                             |
| **Category**     | Classify: `enhancement`, `new-capability`, `integration`, `refactor`|
| **Component**    | Identify the affected area (e.g., TUI, API, CLI, Core Library)      |
| **Priority**     | Estimate: `must-have`, `should-have`, `nice-to-have`                |
| **Summary**      | One-line restatement of the desired outcome                         |

If the description is too vague to understand the user's intent,
**ask clarifying questions immediately** before proceeding to Step 2.

### 2. Load Context

Gather background information:

- **`architecture.md`**: Identify relevant modules, patterns, frameworks, and constraints.
- **`context.md`**: Check for related prior discussions, rejected ideas, or relevant decisions.
- **`git log -n 20`**: Review recent work for anything related to this feature area.
- **Existing code**: Search for any partial implementations, stubs, or `TODO`s related to the feature.

### 3. Research & Evaluate

Investigate how the feature could be built:

#### 3a. Ecosystem Research
- **Search for existing libraries/crates/packages** that could fulfill the requirement.
- **Read documentation** for candidate dependencies (use Context7, docs, or web search).
- **Compare options**: license, maintenance status, compatibility, API ergonomics.

#### 3b. Feasibility Assessment
- **Architectural fit**: Does this align with the current architecture, or does it require changes?
- **Complexity estimate**: Rough sizing — `small` (hours), `medium` (1-2 days), `large` (3+ days).
- **Risk factors**: Breaking changes, performance concerns, new dependency risks.
- **Constraints**: Environment limitations (e.g., no admin rights), compatibility requirements.

#### 3c. Alternatives Analysis
- Identify **at least 2 approaches** where possible (including the user's original idea).
- For each approach, note: pros, cons, complexity, and trade-offs.
- **Recommend** a preferred approach with reasoning.

> [!TIP]
> The goal is to give the user enough information to make an informed decision —
> not to design the full solution (that's for `/plan-making`).

### 4. Produce Feature Research Report

Create a structured report with the following format:

```markdown
## ✨ Feature Research Report

| Field          | Value                                       |
|----------------|---------------------------------------------|
| **Feature**    | [name]                                      |
| **Category**   | enhancement / new-capability / integration  |
| **Component**  | [affected area]                             |
| **Priority**   | must-have / should-have / nice-to-have      |
| **Complexity** | small / medium / large                      |
| **Filed**      | [date]                                      |

### Description
[Clear restatement of the desired feature in the user's own words]

### Current State
- **Existing code:** [relevant modules/files, if any]
- **Related history:** [prior decisions from context.md, if any]
- **Gaps:** [what's missing to support this feature]

### Ecosystem Research
- **Libraries evaluated:** [list with brief notes]
- **Recommended dependency:** [name + reasoning], or "None — custom implementation preferred"

### Approaches

#### Option A: [Name]
- **Description:** [how it works]
- **Pros:** [advantages]
- **Cons:** [disadvantages]
- **Complexity:** [small / medium / large]

#### Option B: [Name]
- **Description:** [how it works]
- **Pros:** [advantages]
- **Cons:** [disadvantages]
- **Complexity:** [small / medium / large]

### Recommendation
[Which option and why. Include any caveats or conditions.]

### Risks & Constraints
- [List risks, trade-offs, and hard constraints]

### Open Questions
- [Ambiguities or decisions that need user input]
```

### 5. Pause for Refinement

End the report with:

> 🛑 **Feature Research Complete.**
> Please review the findings above. You can:
> - **Clarify** or **refine** the feature description
> - **Choose** between the proposed approaches
> - **Adjust** priority or complexity assessment
> - **Add** constraints or requirements not yet captured
>
> When satisfied, reply with **"Plan"** to proceed to `/plan-making`.

**Do NOT proceed to planning until the user explicitly approves the report.**

## Rules

1. **No code edits** — this is a research-only workflow.
2. **No planning** — do not produce implementation steps or blueprints.
3. **Always pause** — the user must explicitly say "Plan" to move forward.
4. **Always suggest alternatives** — even if the user's idea is good, show options.
5. **Ask early** — if the feature is ambiguous, ask questions in Step 1, not Step 4.
6. **Leverage the ecosystem** — check for existing libraries before proposing custom code.
7. **Stay focused** — research just enough to inform a decision; avoid deep prototyping.
