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
- **Issue location:** `<file path>:<line>:<Type::method_name()>` (or entity sentinel `(Module)`, `struct <Name>`)
- **Affected files:** [list of files with links]
- **Root cause analysis:** [diagnosis based on investigation]
- **Impact scope:** narrow (single function) / module / cross-cutting
- **Blast radius & call graph:** [callers, callees, downstream impact from codebase-recon; or "N/A (low severity — lightweight)"]
- **Related history:** [anything from context.md or git log]
- **Recent changes:** [relevant commits, if any]
- **Test coverage:** [existing tests in this area, pass/fail status; or "N/A (lightweight)"]

### Open Questions
- [Any ambiguities or unknowns that need user clarification]

### Recommended Severity
[Confirm or adjust the initial severity estimate with reasoning]
```
<!-- TEMPLATE_END -->

## 3. Investigation Depth

Depth scales with severity:

| Severity | Depth | Required Analysis | Search Mode & Tool Bounds |
|----------|-------|-------------------|---------------------------|
| `critical` / `high` | Full | Blast radius, dependency graph, related history, all test status | MCP AST & graph tools first (`get_callers`, `search_code`); bounded `git log -n 10`; no whole-repo text sweeps by root Architect |
| `medium` | Standard | Affected files, root cause, relevant tests | Targeted symbol lookup (`find_symbols`, `view_file` with line slices); no whole-repo sweeps |
| `low` | Lightweight | Affected files, brief root cause | Scoped inspection of known file/function only; zero broad searches; no subagents spawned |

For **critical/high** issues, the Architect **SHOULD** use `sequentialthinking` to structure
the investigation, evaluate competing hypotheses, and avoid jumping to conclusions.

For **medium/low** issues, skip sequential thinking — the overhead isn't worth it.

## 4. Diagnostic Constraints

1. **No solutions or code edits** — do not propose fixes, architecture changes, implementations,
   or code edits. The Issue Report is a diagnostic input, not a plan.
2. **No planning** — do not draft implementation plans or architectural redesigns during this phase.
3. **Ask early** — if the issue is ambiguous, ask clarifying questions in Step 1 before
   investigating, not after.
4. **Token & search efficiency** — prioritize indexed MCP tools (`search_code`, `find_symbols`,
   `search_knowledge`) before text scanning. When using `grep_search`, restrict to candidate
   subtrees and use `MatchPerLine: false` (filename-only) before full-text queries.
   Cap initial search results to 20 matches maximum.
5. **Bounded inspection** — read code using targeted line slices (`StartLine`/`EndLine` with
   `view_file`). Avoid reading entire files into context. Avoid loading unbounded log streams.
