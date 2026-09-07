---
description: On-demand code review with focused lenses (any time, advisory)
---

# Review Workflow

This workflow provides **qualitative code review** — the kind that catches logic bugs, design smells, security vulnerabilities, and performance bottlenecks that compliance checklists miss.

> [!NOTE]
> `/review` is **advisory** — it produces prioritized findings with severity levels, NOT a pass/fail gate. For formal compliance checks, use `/audit`.

## Workflow Persona

When executing this workflow, adopt the mindset of a **Lead Code Review Architect** coordinating a multi-lens review pipeline. Your role is to parse the review scope, dispatch specialized analysis subagents per lens, and synthesize their per-lens reports into a comprehensive, unified review. You delegate ALL codebase investigation — grep searches, MCP tool calls, file reading, pattern scanning — to specialized subagents running on efficient models. You focus your reasoning exclusively on cross-lens synthesis, severity calibration, deduplication, and actionable prioritization. You do NOT investigate the codebase directly.

## Trigger

`/review [scope] [lens]`

| Argument | Options | Default |
|:---|:---|:---|
| `scope` | File path, `HEAD~N`, `staged`, `all` | `staged` |
| `lens` | `logic`, `design`, `perf`, `security`, `api`, `all` | `all` |

**Examples:**
- `/review` — review staged changes across all lenses
- `/review src/server.rs design` — design smell review of one file
- `/review HEAD~3 logic` — logic review of last 3 commits
- `/review all security` — security-focused sweep of the entire codebase

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed prerequisite files before starting Phase 0. Do not rely on internal memory.

- Read the relevant lens skill(s) from `.gemini/skills/` (or `global/skills/`):
  - `.gemini/skills/review-logic/SKILL.md` (Logic Lens)
  - `.gemini/skills/review-design/SKILL.md` (Design Lens)
  - `.gemini/skills/review-perf/SKILL.md` (Performance Lens)
  - `.gemini/skills/review-security/SKILL.md` (Security Lens)
  - `.gemini/skills/review-api/SKILL.md` (API Lens)
- Read `architecture.md` and `.agents/rules/coding-standard.md` (if present) for project conventions and Language Dispatch Table.
- Confirm you are operating as the **Architect** (no direct code edits).

> [!NOTE]
> **Graceful Fallback**: When `invoke_subagent` is unavailable (single-agent mode), the Architect executes all lens analysis inline — applying the lens checklists directly rather than delegating to subagents. The phase structure remains the same; only the executor changes.

---

## Phases

### Phase 0: Parse Scope, Lens & Tier

Determine what code to review, which lenses to apply, and the execution tier:

1. **Scope Parsing:**
   - **File path** → read target file path(s).
   - **`HEAD~N`** → run `git log -n N --oneline` to find the boundary commit hash, then diff against explicit hash (`git diff <hash> HEAD`).
   - **`staged`** → `git diff --cached --name-only` for staged files.
   - **`all`** → full codebase scan.

2. **Lens Selection:**
   - Single lens: `logic`, `design`, `perf`, `security`, or `api`.
   - `all` → all 5 lenses active.

3. **Scope Tier Decision:**
   - **S-scope**: 1 file + 1 lens → execute inline (no subagent dispatch overhead).
   - **M-scope**: few files + specific lens(es) → dispatch 1–2 specialized subagents.
   - **L-scope**: many files / `all` lenses → dispatch up to 5 specialized subagents in parallel.

### Phase 1: Gather Scope Context

Collect the scoped files and diffs to provide clear boundaries for analysis:
- For staged diffs: run `git diff --cached`
- For commit diffs: run `git diff <hash> HEAD`
- For specific files: identify file paths and verify existence
- If `/review` was triggered following an `/issue`, `/feature`, or `/brainstorm` workflow, extract relevant context from the preceding report to avoid re-investigation.

### Phase 2: Dispatch Lens Subagents

When running in **S-scope**, skip subagent dispatch and evaluate the lens inline.

For **M-scope** and **L-scope**, delegate analysis using the respective standalone review skills:
1. For each active lens, define its specialized subagent via `define_subagent` per its skill:
   - 🔍 `review-logic` → `review_logic` (Senior Software Correctness Engineer)
   - 🏗️ `review-design` → `review_design` (Principal Software Architect)
   - ⚡ `review-perf` → `review_perf` (Senior Performance Engineer)
   - 🔒 `review-security` → `review_security` (Senior Application Security Engineer)
   - 📐 `review-api` → `review_api` (Senior API Design Architect)
2. Announce subagent models per `GEMINI.md §10`:
   > 🤖 Spawning subagent **[Role]** with model: `flash` (Gemini 3.8 Flash High)
3. Invoke each subagent in parallel with the scoped code, checklist questions, and report template instruction.
4. Stop calling tools and wait for all subagents to complete their analysis.

### Phase 3: Architect Synthesis

Once all per-lens reports are returned:
1. **Cross-Lens Correlation:** Identify systemic patterns or code hotspots flagged across multiple lenses (e.g., a function flagged for both logic edge cases and performance allocations).
2. **Severity Calibration:** Normalize severities across reports to ensure uniform judgment:
   - 🔴 **Critical**: Likely bug, exploit path, or data corruption risk
   - 🟠 **Major**: Architecture, maintenance, or scaling blocker
   - 🟡 **Minor**: Non-blocking optimization or ergonomic improvement
   - ⚪ **Nitpick**: Minor naming, formatting, or style preference
3. **Deduplication:** Consolidate redundant observations across lenses into cohesive findings.
4. **Prioritization:** Order findings by severity (Critical first) with concrete, actionable suggestions.

### Phase 4: Produce Unified Review Report

Format the final review as a structured report:

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your chat response (e.g., `[Review Report](file:///absolute/path/to/review_report.md)`).

```markdown
# Review Report

**Scope:** [files/diff reviewed]
**Lens:** [applied lens(es)]
**Date:** [date]
**Review Model:** Subagent-orchestrated (N lens agents dispatched)

## Findings

### [Severity] [Category] — [file:line] — [one-line summary]
**Detail:** [explanation]
**Suggestion:** [actionable improvement with code snippet if applicable]
**Source Lens:** [which specialized subagent identified this]
```

**Severity Scale:**
| Level | Meaning |
|:---|:---|
| 🔴 Critical | Likely bug, security hole, or data loss risk |
| 🟠 Major | Design problem that will cause maintenance pain |
| 🟡 Minor | Improvement opportunity, non-urgent |
| ⚪ Nitpick | Style preference, naming suggestion |

**Categories:** Logic, Design, Performance, Security, API, Readability

### Phase 5: Pause for Discussion

End the review response with:

> 📋 **Review Complete.**
> These findings are advisory — no action is required.
> You can:
> - **Discuss** specific findings
> - **Plan** to address Critical/Major findings via `/plan-making`
> - **Re-run** a specific lens with deeper scope
> - **Dismiss** findings you disagree with

---

## Rules

1. **No code edits** — this is investigation-only.
2. **Advisory, not mandatory** — findings are suggestions, not compliance failures.
3. **Don't duplicate `/audit`** — skip compliance checklists (fmt, clippy, test pass/fail). Focus on qualitative assessment.
4. **Use MCP tools** — subagents leverage Narsil MCP tools for deep AST and graph analysis.
5. **Stay scoped** — review only what was asked. Don't expand to unrelated code.
6. **Be constructive** — every finding should include a suggestion, not just a complaint.
7. **Architect synthesizes, subagents investigate** — when subagents are available, delegate ALL codebase research to specialized lens subagents running on efficient models.
