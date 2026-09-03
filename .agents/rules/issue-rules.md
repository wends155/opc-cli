# Issue Rules

> Loaded by `/issue` workflow. Defines classification criteria, report format, and diagnostic constraints.

## 1. Classification Rubric

### Type

| Type | Definition | Example |
|------|-----------|---------|
| `bug` | Existing behavior is broken or incorrect | Panic on empty input, wrong calculation |
| `feature` | New capability requested | Add CSV export, support dark mode |
| `chore` | Maintenance, refactoring, tooling | Update dependency, fix CI, rename module |
| `docs` | Documentation is missing, wrong, or stale | Outdated architecture.md, missing rustdoc |
| `question` | Clarification needed, no action may be required | "Should this return Option or Result?" |

### Severity

| Severity | Criteria | Response |
|----------|----------|----------|
| `critical` | Production down, data loss, security vulnerability | Investigate immediately, full blast radius |
| `high` | Feature broken, no workaround, blocks user workflow | Full investigation with dependency analysis |
| `medium` | Degraded experience, workaround exists | Standard investigation, affected files + root cause |
| `low` | Cosmetic, minor inconvenience, enhancement | Lightweight investigation, affected files sufficient |

## 2. Issue Report Format

<!-- TEMPLATE_START: issue-report -->
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
- **Affected files:** [list of files with links]
- **Root cause analysis:** [diagnosis based on investigation]
- **Impact scope:** narrow (single function) / module / cross-cutting
- **Related history:** [anything from context.md or git log]
- **Recent changes:** [relevant commits, if any]
- **Test coverage:** [existing tests in this area, pass/fail status]

### Open Questions
- [Any ambiguities or unknowns that need user clarification]

### Recommended Severity
[Confirm or adjust the initial severity estimate with reasoning]
```
<!-- TEMPLATE_END -->

## 3. Investigation Depth

Depth scales with severity:

| Severity | Depth | Required Analysis |
|----------|-------|-------------------|
| `critical` / `high` | Full | Blast radius, dependency graph, related history, all test status |
| `medium` | Standard | Affected files, root cause, relevant tests |
| `low` | Lightweight | Affected files, brief root cause |

For **critical/high** issues, the Architect **SHOULD** use `sequentialthinking` to structure the investigation, evaluate competing hypotheses, and avoid jumping to conclusions.

For **medium/low** issues, skip sequential thinking — the overhead isn't worth it.

## 4. Diagnostic Constraints

1. **No solutions** — do not propose fixes, implementations, or code changes. The Issue Report is a diagnostic input, not a plan.
2. **No code edits** — this is an investigation-only phase. Read files, don't modify them.
3. **No planning** — do not propose architecture changes or implementation steps.
4. **Ask early** — if the issue is ambiguous, ask clarifying questions before investigating, not after.
5. **Stay focused** — investigate just enough to produce a clear report. Avoid rabbit holes.
