---
trigger: model_decision
description: Loaded by `/plan-making` workflow. Defines plan format, revision protocol, and handoff rules.
---

# Implementation Plan Rules (IPR)

> Loaded by `/plan-making` workflow. Defines plan format, revision protocol, and handoff rules.

## 1. The Planning Gate

- **Trigger:** "Plan", "Draft", "Propose", "Design" → Agent is Language-Locked (Markdown only).
- **Prohibited:** Editing source code or core documentation (except to read).
- **Allowed:** Creating/Editing implementation plans in an artifacts directory.
- **Exit:** Agent **MUST** pause and request user approval. Unlock only after "Proceed"/"Approved".
- **Output:** Always an artifact, never code changes.

## 2. Plan Format

> [!CAUTION]
> DO NOT generate plan structures from memory. You MUST load `.gemini/skills/scaffold-plan/SKILL.md` and extract the exact markdown template for the chosen tier.

### Scaling Tiers

| Tier | Files | Required Sections |
|------|-------|-------------------|
| **S** (patch) | 1-3 | Header, Problem Statement, Plan Objectives, Global Execution Order, Verification Plan, Plan Summary |
| **M** (feature) | 4-10 | + Builder Context, Plan Objectives, Interface Contracts, Blast Radius Table, Deprecation Schedule *(if triggered)*, Test Plan, Negative Scope, Phase Context *(if multi-phase)* |
| **L** (refactor) | 10+ | + Plan Objectives, Module Boundaries, Cross-Module Handshakes, Architecture Diagram, Dependency Chain, Blast Radius Table, Deprecation Schedule *(if triggered)*, Phase Manifest *(if multi-phase)* |

### Header

| Field | Value |
|-------|-------|
| **Role** | Architect / Builder |
| **Date** | Current date |
| **Scope** | One-line summary of what changes |
| **Tier** | S / M / L |

> All code produced must comply with `.agents/rules/coding-standard.md`.

### Builder Context *(M/L tier)*

List exact files and line ranges the Builder must read before starting:

```markdown
## Builder Context
Read before starting:
- `src/lib.rs` L1-30 (module structure)
- `src/error.rs` (current error types)
- `architecture.md § Error Handling` (project convention)
```

### Phase Context *(M/L tier, multi-phase only)*

For plans that are part of a multi-phase project (see `phase-rules.md`):

```markdown
## Phase Context
- **Phase:** 2 of 5 (Core Feature)
- **Prior phase:** Phase 1 delivered foundation (config, errors, DB, tracing)
- **Stubs for this phase:** STUB(Phase 2) items from prior Phase Manifest:
  - `MockPaymentGateway` in src/infra/payment.rs → replace with Stripe integration
  - `NoOpNotifier` in src/infra/notify.rs → replace with webhook dispatcher
- **Reference:** See `phase-rules.md` for full conventions
```

> [!NOTE]
> Phase Context is only required for multi-phase plans. Single-phase plans omit this section.
> The Architect determines if a plan is multi-phase during scope analysis in `/plan-making`.

### Problem Statement

- What is the problem or feature request?
- Why does it need to be solved now?
- Any relevant context from `context.md` or prior conversations.
- **Constraints**: Technical limitations, environment restrictions, performance budgets, or scope boundaries.
- **Dependencies**: What existing libraries, crates, or packages can be leveraged? Check the ecosystem before proposing custom solutions.

> [!NOTE]
> If an input report exists from `/issue`, `/audit`, or `/feature`,
> cross-reference each proposed change against the report's findings.
> Every finding should map to a proposed change.

### Plan Objectives

Mandatory for all tiers. Define what "done" looks like — tabular goals with
explicit success criteria. Each objective must be concrete enough that the
Builder can confirm achievement after execution.

**Format:** Table with 4 columns:

| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | <What to achieve> | <How the Builder verifies it — concrete, measurable> | N-M |

- **ID**: Sequential identifier (`O1`, `O2`, ...) for cross-referencing in task.md and audit.
- **Objective**: What the plan delivers — technical, not aspirational.
- **Success Criteria**: How the Builder (or Architect during audit) verifies the objective was met. Must be concrete and measurable — a command to run, a property to check, or a state to observe.
- **Steps**: GEO step range that delivers this objective.

**Good objectives** (concrete, verifiable, Builder-actionable):

| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | Config errors surface at startup | `cargo test test_missing_field` passes; no `unwrap()` in `parse_config()` | 1-3 |
| O2 | All `src/api/` public functions have integration tests | `rg "pub fn" src/api/` count ≤ `rg "#[test]" tests/api/` count | 4-8 |
| O3 | `parse_config()` handles missing fields | `cargo test test_parse_missing` passes; function returns `Result` not `panic` | 1-2 |

**Bad objectives** (vague, aspirational, unmeasurable):
- "Improve system reliability" — no success criteria possible
- "Enhance user experience" — not verifiable by the Builder
- "Deliver value to stakeholders" — not technical

> [!NOTE]
> S-tier plans may have 1–2 rows. M/L-tier plans should have 3+ rows
> covering the primary deliverables and key invariants.

### Negative Scope *(M/L tier)*

Explicitly list what the Builder must NOT touch:

```markdown
## Out of Scope
- Do NOT modify `src/config.rs` (separate plan)
- Do NOT add new dependencies
- Do NOT refactor existing tests
```

### Interface Contracts *(M/L tier)*

For every new or changed public function, struct, or trait:
- Exact signature (name, params, return type, error type).
- Invariants (preconditions, postconditions).
- Error conditions (what can fail and what the caller gets back).

### Blast Radius Table *(M/L tier)*

For every public function, struct, trait, or interface being modified, document
the downstream impact:

| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Package? |
|--------|------|---------------|---------------|-----------|----------------|
| `fn_name()` | `src/mod.rs` | 3 | 1 | 2 | Yes → `package-b` |

> [!TIP]
> Use Narsil `get_callers` (upstream blast radius), `get_callees` (downstream
> dependencies), and `get_call_graph` (full structural view) to populate this
> table when available. Fall back to `find_references` or `rg` when call-graph
> is unavailable.

If any row has `Cross-Package? = Yes`, the Deprecation Protocol applies (see
Deprecation Schedule section).

> [!NOTE]
> Adapt column names to your language ecosystem (e.g., "Cross-Crate?" for Rust,
> "Cross-Module?" for Go, "Cross-Package?" for npm/TypeScript).

### Deprecation Schedule *(when Deprecation Protocol triggers)*

Required when the Blast Radius Table shows cross-package impact, the change
affects a published package, or >5 call sites are affected (even internally).

| Old Symbol | New Symbol | Introduced | Removal Target | Migration |
|-----------|-----------|-----------|---------------|-----------|
| `old_fn()` | `new_fn()` | v0.5.0 | v0.7.0 | See doc comment |

**Deprecation Protocol Rules:**

1. Create the new function with the improved signature.
2. Mark the old function with the language's standard deprecation mechanism
   (e.g., Rust: `#[deprecated(since, note)]`, TypeScript: `@deprecated` JSDoc,
   Go: `// Deprecated:` comment, Python: `warnings.warn(DeprecationWarning)`).
3. Add a migration guide in the new function's doc comment showing before/after.
4. Add `// TODO(deprecation): remove old_fn in v<target>` marker.
5. Update all internal callers in the same PR when feasible.

**Threshold Table:**

| Scenario | Strategy |
|----------|----------|
| Internal-only, ≤5 call sites | Update all callers in same plan |
| Published package or external consumers | Deprecate + migration window |
| >5 call sites (even internal) | Deprecate for incremental migration |
| Interface/trait method with multiple implementations | Always deprecate |

### Module Boundaries *(L tier only)*

For each component group:
- **Owns**: What this module is responsible for.
- **Does NOT own**: What's delegated to other modules.

### Cross-Module Handshakes *(L tier only)*

When a change affects callers/callees across modules:
- Caller → Callee with the exact function/method.
- Data format exchanged (types, ownership).
- Error propagation path across the boundary.

### Global Execution Order *(all tiers)*

> [!IMPORTANT]
> Number ALL steps globally across ALL files. The Builder follows steps 1, 2, 3... linearly.
> No per-file numbering. No jumping.
>
> Line ranges in `(L##-##)` are advisory hints for initial orientation. The
> **Target name** (function, struct, module) is the binding anchor. If prior
> steps shifted line numbers, the Builder locates the target by name using
> `grep_search` or Narsil `get_symbol_definition` — not by stale line range.
>
> [!TIP]
> **Call-Graph-Driven Ordering (M/L tier):** When Narsil call-graph is available,
> use `get_callers`/`get_callees` on each seed function being modified to discover
> the complete modification set. Order steps topologically:
> - **Signature narrowing** (removing params, tightening types): bottom-up
>   (callees first, then callers)
> - **Signature widening** (adding params, new error variants): top-down
>   (callers first, then callees)
> - **New functions**: insert at the dependency layer where they'll be consumed.
>
> Use `find_call_path(A, B)` to verify no indirect paths are missed.

Each step is verification-oriented:

```markdown
Step N: [NEW/MODIFY/DELETE/TEST] file_path — [+/~/−] function_name() (L##-##)
- Pre: CHECK
- Target: function/struct name + line range
- Action: what to change (code snippet or description)
- Post: CHECK, no anyhow in file
- 🔒 CHECKPOINT (only on steps requiring ALL)
```

**Tags:**
- `[NEW]` — create a new file
- `[MODIFY]` — change existing code
- `[DELETE]` — remove a file or code block
- `[TEST]` — write or update a test (TDD Red step)

> [!WARNING]
> Steps that `[DELETE]` test files or use `[-]` on test functions require an
> explicit justification in the Action description. Test regression without
> justification will be flagged as a Fidelity Matrix violation during `/audit`
> (see `audit-rules.md §4: Test Regression`).

**Function sub-tags** *(optional S-tier, recommended M-tier, required L-tier):*
- `[+]` — new symbol (function, struct, trait) being added
- `[~]` — existing symbol being modified
- `[-]` — symbol being removed from the file

When present, the sub-tag **overrides** the file-level tag for Action Specificity:
- `[MODIFY] — [+]` follows `[NEW]` specificity (full code body)
- `[MODIFY] — [~]` follows `[MODIFY]` specificity (code snippet / prose)
- `[MODIFY] — [-]` follows `[DELETE]` specificity (prose, name the target)

**Action specificity rules:**

| Step Type | Size | Architect Must Provide |
|-----------|------|----------------------|
| `[NEW]` | ≤ 30 lines | Full code body |
| `[NEW]` | 31–80 lines | All public signatures + core logic paths; internal helpers in prose with structural guidance |
| `[NEW]` | > 80 lines | **Decompose into multiple steps** (each ≤ 30 lines targeting a specific function/block) — *unless* the file is purely mechanical (schema DDL, config, test fixtures), in which case full body is permitted |
| `[TEST]` | Any | Full test body (tests are the contract — never prose) |
| `[MODIFY]` | ≤ 20 changed lines | Code snippet showing the replacement |
| `[MODIFY]` | > 20 changed lines | Prose + **all new/changed function signatures** (see Control Flow Override below) |
| `[DELETE]` | Any | Prose — name the target to remove |

**Placeholder convention:** Skeleton code uses `// STUB(Phase N): description`
markers per `phase-rules.md`. Do NOT use `todo!()` or `unimplemented!()`.

**Control Flow Override:**

Whether a large `[MODIFY]` step needs code depends on what kind of change it
makes. **If the change alters the number or meaning of code paths through the
function, include a code snippet regardless of size.**

**Code required** (alters code paths):

| Change Type | Why code is needed |
|-------------|-------------------|
| Adding/removing `match` arms | Pattern matching is precise — arm order and exhaustiveness matter |
| Adding/removing `if`/`else` branches | Branch conditions are design decisions |
| Adding/removing `?` error propagation | Error paths are interface contracts |
| Changing function return type | Signature is a contract — every call site is affected |
| Adding `async`/`await` | Concurrency model is architectural |
| Loop refactoring (changing iteration) | Iteration boundaries are logic, not style |

**Prose sufficient** (structural/mechanical):

| Change Type | Why prose works |
|-------------|----------------|
| Renaming symbols | Find-replace with a rename mapping |
| Moving code between modules | Source path → destination path |
| Adding doc comments | Documentation content in prose |
| Adding `#[derive(...)]` / attributes | List the additions |
| Deleting dead code | Name the targets to remove |
| Adding imports | List the imports |

> Quick test for Architects: **"Does this change alter the number or meaning of
> code paths through the function?"** If yes → include code. If no → prose is fine.

**Pre/Post Vocabulary:**

| Shorthand | Meaning | Default Command |
|-----------|---------|-----------------|
| `CHECK` | Type-check compiles | `cargo check` |
| `FMT` | Format passes | `cargo fmt --check` |
| `CLIPPY` | Lint passes | `cargo clippy -- -D warnings` |
| `TEST` | Tests pass | `cargo test` |
| `BUILD` | Full build | `cargo build` |
| `ALL` | FMT + CLIPPY + TEST | Full pipeline |
| `RED` | Named test compiles but fails | *Per `architecture.md § Toolchain`* |
| `GREEN` | Previously-RED test now passes | *Per `architecture.md § Toolchain`* |

> [!NOTE]
> Default commands shown. Projects override in `architecture.md § Toolchain`.

**`RED` / `GREEN` semantics:**

- `RED(test_name)`: The named test **compiles** but **fails** (exit non-zero).
  The Pre condition (typically `ALL`) already guarantees the rest of the suite
  is green. If the test doesn't compile, that's a bug in the test code —
  Builder fixes it before proceeding.
- `GREEN(test_name)`: The named test **passes** (exit 0). Semantically
  equivalent to `TEST` but communicates the TDD intent — this is the
  Red→Green transition, not a generic test run.

Pre/Post can combine shorthand with conditions: `Post: CHECK, no anyhow imports in file`.

Free-form Post conditions **MUST** include the verification command with an
`expects:` clause in parentheses.

| Notation | Meaning |
|----------|---------|
| `expects: 0 matches` | Search returns no results |
| `expects: ≥1 match` | Search returns at least one result |
| `expects: exit 0` | Command succeeds |
| `expects: exit non-zero` | Command fails (expected failure) |

**Examples:**

✅ `Post: CHECK, no anyhow usage (rg "anyhow" src/config.rs expects: 0 matches)`
✅ `Post: CHECK, uses thiserror (rg "thiserror" src/error.rs expects: ≥1 match)`
❌ `Post: CHECK, no anyhow in file`
❌ `Post: CHECK, error handling looks correct`

**🔒 CHECKPOINT** marks where the Builder runs `ALL` and commits.

**Checkpoint frequency:** At minimum, place `🔒` after each component group and after every `[TEST]` step. For S-tier plans, one `🔒` at the end suffices.

> A **component group** is all contiguous steps targeting the same crate (Rust),
> package (TS/Go), or top-level module. When the plan's execution order crosses
> a crate/package boundary, place a 🔒 CHECKPOINT before entering the next
> boundary.

#### Parallel Execution Lanes *(L-tier, optional — Multi-Agent only)*

When multi-agent orchestration is available (detected by `/toolcheck`) AND the
Architect declares parallel lanes in the plan:

- **Lane syntax**: Group steps into named lanes, each scoped to a module boundary:
  ```
  Lane A (Steps N-M): `src/module_a/` — description
  Lane B (Steps N-M): `src/module_b/` — description
  🔒 SYNC CHECKPOINT — integrate and verify all lanes
  ```
- Steps within a lane execute **sequentially**; lanes execute **concurrently**.
- Each lane has its own local `🔒` checkpoints for lane-scoped verification.
- The `🔒 SYNC CHECKPOINT` at lane convergence runs the FULL `ALL` pipeline
  on the integrated workspace.
- **No file may appear in more than one lane** (strict module partitioning).
- If multi-agent orchestration is NOT available, ignore this section entirely.
  The standard sequential GEO applies.

### Dependency Chain *(L tier)*

Show which steps depend on which:

```
1 → 2 → 3
         ↘
     4 → 5 → 6 🔒
```

### Architecture Diagram *(if applicable, M/L tier)*

Include a Mermaid diagram for any structural or data-flow changes.

### Edge Cases & Risks

List edge cases the implementation must handle. Document risks or trade-offs.

### Test Plan (TDD) *(M/L tier)*

> [!IMPORTANT]
> Plans **must** specify tests **before** implementation code. The Builder writes
> tests first, verifies they fail (Red), then implements until they pass (Green).

For each proposed change, define:

1. **Test cases**: Function signatures and assertions — written as executable code, not prose.
2. **Test type**: Unit, integration, property-based, or doc-test.
3. **Expected failures**: What the test asserts when run *before* implementation.
4. **Test file location**: Co-located `#[cfg(test)]` module or dedicated test file.

**Code snippets as executable tests:** Instead of describing expected output in prose,
express verification as a test assertion. The plan's code should be testable, not illustrative.

### Verification Plan *(all tiers)*

| Type | Required? | Details |
|------|-----------|---------|
| **Automated tests** | Yes | Exact command (e.g., `cargo test`, `npm test`) |
| **Lint / Format** | Yes | Exact command (e.g., `cargo fmt --check`) |
| **Manual testing** | If applicable | Step-by-step instructions |
| **Browser testing** | If applicable | Specific pages/flows |

> [!IMPORTANT]
> Do NOT invent test commands. Refer to `architecture.md § Toolchain`.

### Plan Summary *(all tiers)*

| Metric | Value |
|--------|-------|
| Tier | S / M / L |
| Files | N |
| Steps | N |
| Checkpoints | N |
| Estimated effort | Low / Medium / High |

## 3. Revision Protocol

> 📘 **Skill:** [`scaffold-plan`](../../.gemini/skills/scaffold-plan/SKILL.md) — Load this skill when instructed to review and update an existing plan.

## 4. Decision Resolution

> 📘 **Skill:** [`scaffold-plan`](../../.gemini/skills/scaffold-plan/SKILL.md) — Load this skill when asked to finalize a drafted plan by resolving open decisions or selecting among alternatives.

## 5. Handoff-Ready Requirements

Before the Architect can request "Proceed", the plan must satisfy:

| Requirement | Verification |
|-------------|-------------|
| Every file listed with `[NEW]`/`[MODIFY]`/`[DELETE]`/`[TEST]` tags | Manual review |
| Every change has a discrete, verifiable description | Manual review |
| Test cases pre-specified (TDD: Red → Green → Refactor) | Test Plan section exists |
| Verification commands sourced from `architecture.md § Toolchain` | Cross-reference check |
| Plan Summary filled in | Manual review |
| `task.md` aligned | Pre-Flight Gate (Validate task.md procedure) |

## 6. The task.md Contract

`task.md` is the bridge between the Architect's plan and the Builder's execution:

1. **Generated** by the Architect during the `/plan-making` workflow.
2. **1:1 Mapping**: Each checklist item maps to exactly one plan item.
3. **Progress Tracking**: Builder marks `[ ]` → `[/]` (in-progress) → `[x]` (done).
4. **Validation Gate**: Before each commit, the Builder MUST verify that all modified files have a corresponding `[x]` entry in `task.md`.
5. **Parallel Lane Partitioning** *(optional — Multi-Agent only)*: When a plan declares Parallel Execution Lanes, the Architect generates per-lane sub-task files (`task_lane_a.md`, `task_lane_b.md`). The main `task.md` retains a summary line per lane linking to the sub-file. Each subagent updates ONLY its assigned sub-task file to prevent write conflicts.

## 7. Builder Obligations & STOP Conditions

**Obligations:**
1. Execute plan items in order — no deviations.
2. If a plan item is unclear or flawed → **STOP**, request re-audit.
3. Update `task.md` in the artifacts directory after each file modification:
   - Mark `[ ]` → `[/]` when starting a step.
   - Mark `[/]` → `[x]` when the step passes verification.
   - Antigravity reads this file for UI progress — stale markers hide progress from the user.
4. Run `ALL` at each 🔒 CHECKPOINT.
5. Emulate `Git-Checkpoint.ps1` behavior natively: ensure `task.md` is updated, then run `git add .` and `git commit -m "..."`.
6. 🛑 **Wait for user instruction** before pushing to remote repositories.

**STOP Conditions** — Builder must immediately halt and return to the Architect when:
- The plan contradicts `architecture.md`.
- A plan item is ambiguous or untestable.
- An unplanned dependency or breaking change is discovered.
- The second consecutive test failure occurs on the same item.

**On STOP:** Commit current progress with message `WIP: stopped at step N — [reason]`.
Do NOT revert completed steps. The Architect decides rollback scope during re-planning.
If a step broke prior work, note the regression in the STOP report.

## 8. Resumption Protocol

> 📘 **Skill:** [`scaffold-plan`](../../.gemini/skills/scaffold-plan/SKILL.md) — Load this skill when starting a new session to resume execution of an approved plan.


