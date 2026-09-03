---
description: Create, update, or audit spec.md behavioral contracts (Pre-Think Phase)
---

# /spec — Behavioral Specification Management

Create, update, or audit a project's behavioral contracts in `spec.md`.

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed rule files (e.g., `.agents/rules/...`) before starting Step 1. Do not rely on internal memory.

- Read `.agents/rules/spec-rules.md` for required sections, BDD conventions, and templates.
- Read `architecture.md` (**required** — module boundaries are the input for behavioral contracts).
- Read `context.md` (if present) for historical decisions.
- Read `spec.md` (if present) to assess current state.
- Confirm you are operating in **Planning mode** (no code edits allowed).

> [!IMPORTANT]
> `architecture.md` is a **hard prerequisite**. You cannot write behavioral contracts
> for modules that aren't defined. If `architecture.md` doesn't exist, run `/architecture` first.

## Step 1: Assess Project State

Determine the workflow mode by checking two conditions:

| spec.md exists? | architecture.md exists? | Mode |
|-----------------|------------------------|------|
| No | Yes | **New** — create from architecture module boundaries |
| No | No | **Defer** — run `/architecture` first |
| Yes | Yes | **Update** — add/change contracts for new/changed modules |
| Yes | * | **Audit** — coverage report + drift check |

---

## New Path (architecture.md exists, no spec.md)

### Step 2N: Extract Module Inventory

From `architecture.md`, extract:
1. All modules listed in Module Boundaries (with Owns/Does NOT own)
2. Public trait interfaces at module boundaries
3. Data models (if Data Model section exists)
4. State machines (if any are described)
5. CLI/API interfaces (if applicable)

#### MCP-Enhanced Extraction *(when available)*

If **Narsil MCP** is available, supplement architecture.md with actual code analysis:

| Tool | Purpose |
|------|---------|
| `get_callers` | Determine caller-side invariant requirements (who depends on this contract) |
| `get_callees` | Determine downstream preconditions (what this module requires) |
| `find_symbols` (type: function, trait, struct) | Discover public API surface per module |
| `get_export_map` | Verify which symbols are actually exported |
| `get_symbol_definition` | Get signatures for contract writing |
| `get_control_flow` | Understand state transitions in stateful components |

### Step 3N: Draft spec.md

For each module identified in Step 2N:

1. Write the **Metadata Header** per `spec-rules.md §7`
2. For each module with public API:
   - Create a **Module Contract** per `spec-rules.md §3` template
   - Write BDD scenarios (minimum: 1 `[HAPPY]`, 1 `[ERROR]` per public function)
   - List invariants and required test coverage
3. For each data model:
   - Create a **Data Model** entry per `spec-rules.md §4`
   - Add BDD scenarios for complex validation rules
4. For each stateful component:
   - Create a **State Machine** entry per `spec-rules.md §5`
   - Every transition must have trigger + side effects
5. For CLI/API interfaces:
   - Create contracts per `spec-rules.md §6`

For multi-module projects, the Architect **SHOULD** use `sequentialthinking` to reason about behavioral boundaries, cross-module contracts, and scenario coverage before drafting.

### Step 4N: Present for Review

Present the draft `spec.md` to the user.

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your final chat response (e.g., `[spec.md](file:///absolute/path/to/spec.md)`).

The user responds with:
- **Feedback** → revise specific sections, keep approved parts unchanged
- **"Plan"** → route to `/plan-making` if implementation work is needed based on gaps found

---

## Update Path (spec.md + architecture.md exist)

### Step 2U: Identify Gaps

Compare `architecture.md` modules against `spec.md` coverage:

1. List modules in `architecture.md` not yet in `spec.md`
2. List modules whose public API changed (new functions, new params, new error types)
3. List data models added or modified
4. List state machines added or modified

#### MCP-Enhanced Gap Detection *(when available)*

| Tool | Purpose |
|------|---------|
| `find_symbols` | Compare current public API against spec.md contracts |
| `get_callers` | Verify spec'd functions still have consumers; detect orphaned contracts |
| `get_symbol_definition` | Verify spec'd signatures match actual code |

### Step 3U: Draft New Contracts

For each gap identified in Step 2U:

1. Draft new module contracts using `spec-rules.md §3` template
2. Draft new data model entries using `spec-rules.md §4`
3. Draft new state machine entries using `spec-rules.md §5`
4. Present additions to user — do NOT modify existing approved contracts without explicit request

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your final chat response (e.g., `[spec.md](file:///absolute/path/to/spec.md)`).

> [!NOTE]
> Update mode adds contracts for **new** modules. To modify existing contracts,
> the user must explicitly request it or an Audit must flag drift.

### Step 4U: Present for Review

Same gate as New path — user reviews, provides feedback, or says "Plan".

---

## Audit Path (spec.md exists, check coverage and drift)

### Step 2A: Scan Coverage

Cross-reference `spec.md` against `architecture.md` and actual code:

| Check | How |
|-------|-----|
| Module coverage | Every module in `architecture.md` has a contract in `spec.md` |
| API coverage | Every public function in each module has at least 1 `[HAPPY]` + 1 `[ERROR]` scenario |
| Data model coverage | Every struct in Data Model section matches code |
| State machine accuracy | Declared transitions match actual code paths |
| Signature drift | spec.md signatures match current code signatures |
| Stale contracts | Contracts for modules/functions that no longer exist |

#### MCP-Enhanced Audit *(when available)*

| Tool | Purpose |
|------|---------|
| `find_symbols` | Compare actual public API against spec.md contracts |
| `get_symbol_definition` | Verify signatures haven't drifted |
| `get_callers`, `get_callees` | Verify structural usage matches spec'd consumption |
| `get_call_graph` | Verify declared module interactions in spec.md match actual structural dependencies |
| `get_control_flow` | Verify state machine transitions are accurate |
| `check_cwe_top25` | Identify security-sensitive code needing `[SECURITY]` scenarios |

### Step 3A: Generate Coverage Report

Output a **Spec Coverage Report** — do NOT edit `spec.md` directly.

```markdown
## Spec Coverage Report

| Field | Value |
|-------|-------|
| Date | [current date] |
| Mode | Audit |
| Modules in architecture.md | N |
| Modules in spec.md | N |
| Coverage | N% |

### Missing Contracts
| Module | Public Functions | Status |
|--------|-----------------|--------|
| [module] | [N functions] | Not in spec.md |

### Signature Drift
| Module | Function | spec.md Signature | Actual Signature |
|--------|----------|-------------------|-----------------|
| [module] | [fn] | [old] | [new] |

### Stale Contracts
- [contracts referencing non-existent modules/functions]

### Missing Scenario Coverage
| Module | Function | Has [HAPPY]? | Has [ERROR]? | Has [SECURITY]? |
|--------|----------|-------------|-------------|-----------------|

### Recommendations
1. [ordered list of suggested changes]
```

**Gate:** Present report to user. User responds with:
- **"Plan"** → route to `/plan-making` to implement recommendations
- **"Accept"** → no changes needed

---

## Step 5 (all paths): Validate

1. Cross-reference the draft/report against `spec-rules.md §1` required sections
2. Verify every module contract follows `spec-rules.md §3` template
3. Verify BDD scenarios use correct tags (`[HAPPY]`, `[ERROR]`, `[EDGE]`, `[SECURITY]`)
4. Verify data models use field table format from `spec-rules.md §4`
5. Verify state machines have both diagram and transition table per `spec-rules.md §5`

## Rules

- This is a **Planning-mode workflow** — no source code changes allowed.
- Output: `spec.md` (New/Update) or Coverage Report (Audit).
- `architecture.md` is a **hard prerequisite** — defer if it doesn't exist.
- If drift is found during Audit, **document it** — do not fix. Fixes go through `/plan-making`.
- Drift detection mechanics (hash-based tracking) are owned by `doc-rules.md §5` and `/update-doc`, not this workflow.


