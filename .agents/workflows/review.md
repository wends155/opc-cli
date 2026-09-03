---
description: On-demand code review with focused lenses (any time, advisory)
---

# Review Workflow

This workflow provides **qualitative code review** — the kind that catches logic
bugs, design smells, and performance issues that compliance checklists miss.

> [!NOTE]
> `/review` is **advisory** — it produces findings with severity levels,
> NOT a pass/fail gate. For formal compliance checks, use `/audit`.

## Trigger

`/review [scope] [lens]`

| Argument | Options | Default |
|----------|---------|---------|
| `scope` | File path, `HEAD~N`, `staged`, `all` | `staged` |
| `lens` | `logic`, `design`, `perf`, `security`, `api`, `all` | `all` |

**Examples:**
- `/review` — review staged changes, all lenses
- `/review src/server.rs design` — design smell review of one file
- `/review HEAD~3 logic` — logic review of last 3 commits
- `/review all security` — security-focused review of entire codebase

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed rule files (e.g., `.agents/rules/...`) before starting Step 1. Do not rely on internal memory.

- Read `architecture.md` and `.agents/rules/coding-standard.md` (if present) for governance core rules. Next, check its Language Dispatch Table to determine which language skill files from `.gemini/skills/` to read based on the task's language.
- Confirm you are operating as the **Architect** (no code edits).

## Steps

### 1. Parse Scope & Lens

Determine what code to review and which lens to apply:

- **File path** → read the file(s) directly.
- **`HEAD~N`** → run `git log -n N --oneline` to find the boundary commit hash, then `git diff <hash> HEAD` using the explicit hash (the `~` character is banned by the IDE).
- **`staged`** → `git diff --cached --name-only` for staged files.
- **`all`** → full codebase scan (use Narsil `get_project_structure` if available).

### 2. Gather Code

Read the scoped files. For diff-based scopes, focus on changed regions but
read enough surrounding context to understand the logic.

// turbo
> [!TIP]
> For diff-based scopes, run:
// turbo
> - `git diff --cached --name-only` (staged)
// turbo
> - `git log -n N --name-only --oneline` (recent commits — `~` is banned by IDE; use explicit hashes for diffs)

### 3. Apply Review Lenses

Apply the selected lens (or all lenses). Each lens has specific questions to answer.

---

#### 🔍 Logic Lens

*Does the code do what it's supposed to do?*

- [ ] Algorithm correctness — are there off-by-one errors, edge cases, or logic gaps?
- [ ] Control flow — are all branches reachable? Are early returns used appropriately?
- [ ] Error paths — what happens on failure? Are errors propagated or swallowed?
- [ ] Boundary conditions — empty inputs, max values, concurrent access?
- [ ] State management — are invariants maintained across mutations?

**MCP tools:** `get_control_flow`, `get_data_flow`, `find_dead_code`, `find_uninitialized`

---

#### 🏗️ Design Lens

*Is the code well-structured and maintainable?*

- [ ] **Coupling** — does this module depend on too many others? Would a change here ripple?
- [ ] **Cohesion** — does each module/struct have a single, clear responsibility?
- [ ] **SOLID violations** — Single Responsibility, Open/Closed, Liskov, Interface Segregation, Dependency Inversion?
- [ ] **Mockability** — are dependencies behind traits? Can this be tested in isolation?
- [ ] **Abstraction level** — is the code at the right level of abstraction? Too granular? Too broad?
- [ ] **God objects** — any struct/module doing too much?
- [ ] **Feature envy** — does a function mostly operate on another struct's data?

**MCP tools:** `get_import_graph`, `find_circular_imports`, `get_dependencies`, `find_references`

---

#### ⚡ Performance Lens

*Are there unnecessary costs?*

- [ ] **Allocations** — unnecessary `clone()`, `to_string()`, `collect()` where iterators suffice?
- [ ] **Complexity** — O(n²) loops where O(n) or O(n log n) is possible?
- [ ] **Lock contention** — holding locks across async boundaries or I/O?
- [ ] **N+1 queries** — database calls in loops?
- [ ] **Unbounded growth** — collections that grow without limits?
- [ ] **Hot path** — is the critical path optimized? Are cold paths acceptably slow?

**MCP tools:** `get_data_flow`, `search_code` for `.clone()`, `find_similar_code`

---

#### 🔒 Security Lens

*Could this be exploited?*

- [ ] **Input validation** — is all external input validated/sanitized?
- [ ] **Auth boundaries** — are authorization checks in place for sensitive operations?
- [ ] **Injection** — SQL, command, path traversal risks?
- [ ] **Secrets** — credentials in code, logs, or error messages?
- [ ] **Trust boundaries** — is data from untrusted sources treated differently?
- [ ] **Cryptography** — are secure algorithms and random sources used?

**MCP tools:** `scan_security`, `check_owasp_top10`, `check_cwe_top25`, `find_injection_vulnerabilities`, `get_taint_sources`

---

#### 📐 API Lens

*Is the public interface ergonomic and correct?*

- [ ] **Naming** — are function/type names intuitive and consistent?
- [ ] **Error types** — are errors informative? Can the caller distinguish failure modes?
- [ ] **Builder pattern** — are complex constructors using builders where appropriate?
- [ ] **Type safety** — are newtypes used to prevent primitive obsession?
- [ ] **Documentation** — do public items have doc comments with examples?
- [ ] **Backwards compatibility** — would this change break existing callers?

**MCP tools:** `find_symbols`, `get_symbol_definition`, `get_export_map`, `find_symbol_usages`

---

### 4. Produce Review Report

Structure findings as:

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your final chat response (e.g., `[Review Report](file:///absolute/path/to/review_report.md)`).

```markdown
# Review Report

**Scope:** [files/diff reviewed]
**Lens:** [applied lens(es)]
**Date:** [date]

## Findings

### [Severity] [Category] — [file:line] — [one-line summary]
**Detail:** [explanation]
**Suggestion:** [what to consider]
```

**Severity levels:**
| Level | Meaning |
|-------|---------|
| 🔴 Critical | Likely bug, security hole, or data loss risk |
| 🟠 Major | Design problem that will cause maintenance pain |
| 🟡 Minor | Improvement opportunity, non-urgent |
| ⚪ Nitpick | Style preference, naming suggestion |

**Categories:** Logic, Design, Performance, Security, API, Readability

### 5. Pause for Discussion

End the report with:

> 📋 **Review Complete.**
> These findings are advisory — no action is required.
> You can:
> - **Discuss** specific findings
> - **Plan** to address Critical/Major findings via `/plan-making`
> - **Dismiss** findings you disagree with

## Rules

1. **No code edits** — this is investigation-only.
2. **Advisory, not mandatory** — findings are suggestions, not compliance failures.
3. **Don't duplicate `/audit`** — skip compliance checklists (fmt, clippy, test pass/fail). Focus on qualitative assessment.
4. **Use MCP tools** when available for deeper analysis.
5. **Stay scoped** — review only what was asked. Don't expand to unrelated code.
6. **Be constructive** — every finding should include a suggestion, not just a complaint.


