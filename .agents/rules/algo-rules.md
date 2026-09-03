---
description: Algorithm documentation standards and integration contract format (Loaded by /ago)
---

# Algorithm Rules

> Loaded by the `/ago` workflow. Defines required sections for `algorithm.md`,
> integration contract format, and coverage rules.

## 1. Per-Algorithm Sections (Required)

Every algorithm documented in `algorithm.md` must include these 8 sections:

| # | Section | Purpose |
|---|---------|---------|
| 1 | **Problem Statement** | Formal definition: constraints, objective function, input/output types |
| 2 | **Algorithm Candidates** | Viable approaches with citations or references |
| 3 | **Complexity Analysis** | Time/space for each candidate — Big-O and practical at target N |
| 4 | **Tradeoff Matrix** | Compare: optimality, runtime, implementation effort, maintainability |
| 5 | **Selected Approach** | Decision with rationale — why this over the alternatives |
| 6 | **Pseudocode** | Step-by-step logic (not implementation code) |
| 7 | **Edge Cases** | Boundary conditions, degenerate inputs, failure modes |
| 8 | **Integration Contract** | System placement — see §3 for required fields |

## 2. Multi-Algorithm Sections (When Applicable)

When `algorithm.md` documents more than one algorithm:

| Section | Purpose |
|---------|---------|
| **Algorithm Interaction Graph** | Mermaid diagram showing data flow between algorithms |
| **Pipeline Ordering** | Execution sequence, dependencies, parallelism opportunities |
| **Shared Data Structures** | Common types used across algorithms |

## 3. Integration Contract Format

Every algorithm must declare how it integrates with the system. This contract is
consumed by the `/architecture` workflow to prevent orphaning.

```markdown
### Integration Contract

| Field | Value |
|-------|-------|
| **Module Home** | `routing::solver` |
| **Input** | `orders::OrderBatch` from `orders` module |
| **Output** | `routing::RoutePlan` consumed by `dispatch` module |
| **Error Surface** | `SolverError::Infeasible` — no valid route exists |
| **Performance Budget** | < 500ms for N ≤ 1000, < 5s for N ≤ 10000 |
| **Fallback** | Return nearest-neighbor greedy solution on timeout |
```

**Required fields:**
- **Module Home** — where this algorithm lives in the codebase
- **Input** — data type + source module
- **Output** — data type + consuming module(s)
- **Error Surface** — what can fail and the error type
- **Performance Budget** — max latency/memory the system tolerates
- **Fallback** — behavior on failure or timeout

> [!IMPORTANT]
> The `/architecture` workflow verifies that every module referenced in an
> integration contract (source, consumer) exists in the system's module boundaries.
> Missing modules are flagged as **orphan risks**.

## 4. Coverage Rules

| Rule | Requirement |
|------|-------------|
| Every algorithm | Must have all 8 per-algorithm sections |
| Integration contract | Must reference real modules (verified by `/architecture`) |
| Complexity analysis | Must include both theoretical (Big-O) and practical (at target N) |
| Tradeoff matrix | Must compare at least 2 candidates (even if choice is obvious) |
| Pseudocode | Must be language-agnostic — no implementation details |
| Edge cases | Must cover: empty input, single element, maximum scale, invalid input |

## 5. Algorithm Categories

Reference for problem classification during Step 1 of `/ago`:

| Category | Examples |
|----------|---------|
| **Optimization** | Linear programming, genetic algorithms, simulated annealing |
| **Graph** | Shortest path, MST, network flow, matching |
| **Constraint Satisfaction** | Scheduling, timetabling, resource allocation |
| **Search** | A*, branch-and-bound, beam search |
| **Sorting / Selection** | Custom comparators, top-K, partial sorting |
| **Machine Learning** | Classification, regression, clustering pipelines |
| **String / Text** | Parsing, pattern matching, fuzzy search |
| **Numerical** | Signal processing, interpolation, simulation |
