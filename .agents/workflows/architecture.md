---
description: Map project architecture, create or audit architecture.md (Pre-Think Phase)
---

# /architecture — Project Architecture Mapping

Create, discover, or audit a project's architectural documentation.

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed rule files (e.g., `.agents/rules/...`) before starting Step 1. Do not rely on internal memory.

- Read `.agents/rules/architecture-rules.md` for required sections and conventions.
- Read `context.md` (if present) for historical decisions.
- Read `algorithm.md` (if present) for algorithm integration contracts — place each algorithm in the module boundary map.
- Read `design/design-spec.md` (if present) for UI/UX constraints that affect system structure.
- Confirm you are operating in **Planning mode** (no code edits allowed).

> [!TIP]
> Scan project state using native agent tools (zero-prompt):
> 1. Read `Cargo.toml`, `package.json`, or `go.mod` with `view_file`.
> 2. Use `find_by_name` to list source files and config files (`rustfmt.toml`, `clippy.toml`).
// turbo
> 3. `git log -n 10 --oneline` — recent changes for context.

## Step 1: Assess Project State

Determine the workflow mode by checking two conditions:

| Source code exists? | architecture.md exists? | Mode |
|---------------------|------------------------|------|
| No | No | **New** — design from requirements |
| Yes | No | **Discover** — scan and document |
| Yes | Yes | **Audit** — compare and recommend |

---

## New Path (no source code)

### Step 2N: Gather Requirements

Ask the user:
- What is the project's purpose?
- What are the project's primary objectives and key features?
- What language and framework?
- What are the expected modules / components?
- What external systems will it interact with (databases, APIs, queues)?
- If databases: what type (SQL, NoSQL)? Expected tables/collections?
- Are there specific architectural patterns preferred (Clean, Hexagonal, etc.)?

### Step 3N: Design Module Structure

Using the gathered requirements:
1. Propose modules with Owns / Does NOT own declarations.
2. Define trait interfaces for inter-module dependencies.
3. Specify mockability for each boundary.
4. Build the Dependency Direction Rules table (Module → May Import → Must NOT Import).
5. Generate architecture diagrams (Mermaid):
   - Module interaction diagram
   - Data flow diagram
6. **If `algorithm.md` exists**: verify each algorithm's integration contract.
   - Confirm the declared Module Home exists in the proposed module structure.
   - Confirm Input source modules and Output consumer modules exist.
   - Flag **orphan risks**: algorithms referencing modules that don't exist.

For multi-module projects, the Architect **SHOULD** use `sequentialthinking` to reason about module boundaries, dependency direction, and layer separation before drafting.

### Step 4N: Draft architecture.md

Fill all 15 required sections from `architecture-rules.md` §1.
Include Data Model section per `architecture-rules.md` §6 if persistent storage is used.
Mark sections that can't be determined yet as **TBD**.
Present draft to user for review.

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your final chat response (e.g., `[architecture.md](file:///absolute/path/to/architecture.md)`).

---

## Discover Path (code exists, no architecture.md)

### Step 2D: Scan the Project

Use Narsil tools for comprehensive code analysis:

| Tool | Purpose |
|------|---------|
| `get_project_structure` | Directory layout and file organization |
| `get_import_graph` | Module dependency graph |
| `find_symbols` | Public functions, structs, traits per module |
| `find_circular_imports` | Detect circular dependencies |
| `get_export_map` | Public API surface per module |

Also scan for data model indicators:
- Migration folders (`migrations/`, `db/migrate/`)
- SQL files, schema definitions
- ORM config (diesel.toml, prisma schema, sqlx queries)
- Database connection setup code

// turbo
Also: `git log -n 10 --oneline` for recent changes and `view_file` on `Cargo.toml` for project metadata.

### Step 3D: Map Boundaries & Dependencies

For each discovered module:
1. Infer Owns / Does NOT own from actual code.
2. Identify trait interfaces used at boundaries.
3. Assess mock availability — does a mock impl exist? Should one?
4. Build dependency direction table from actual import graph.
5. Flag violations: circular imports, infrastructure imported at handler level.
6. **If `algorithm.md` exists**: cross-reference integration contracts with discovered modules.
   - Verify each algorithm's Module Home maps to an actual code module.
   - Verify Input/Output interfaces match real types in the codebase.
   - Flag **orphaned algorithms**: contracts referencing non-existent modules or types.

For multi-module projects, the Architect **SHOULD** use `sequentialthinking` to reason about boundary classification and identify hidden coupling.

### Step 4D: Generate Diagrams & Draft

1. Generate Mermaid diagrams:
   - Module interaction diagram (from import graph)
   - Data flow diagram (trace data from entry points to storage)
   - Error propagation diagram (if error types are visible)
2. Fill all 15 required sections from `architecture-rules.md` §1.
3. Include Data Model section per `architecture-rules.md` §6 if persistent storage was detected.
4. Present draft to user for review.

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your final chat response (e.g., `[architecture.md](file:///absolute/path/to/architecture.md)`).

---

## Audit Path (code + existing architecture.md)

### Step 2A: Scan the Project

Same as Discover path — full Narsil scan + `Scan-ProjectDocs.ps1 -Mode scan`.

### Step 3A: Compare Declared vs Actual

Cross-reference `architecture.md` against actual code:

| Check | How |
|-------|-----|
| Module Boundaries | Compare declared Owns/Does NOT own against actual code in each module |
| Dependency Direction | Compare declared May/Must NOT Import against actual import graph |
| Trait Interfaces | Verify declared traits exist and are used at boundaries |
| Mock Availability | Check if declared mocks actually exist |
| Undocumented Modules | Find modules in code not listed in architecture.md |
| Stale Sections | Find sections describing code that no longer exists |
| Missing Sections | Check which of the 15 required sections are absent |

### Step 4A: Generate Recommendations Report

Output an **Architecture Recommendations Report** — do NOT edit architecture.md directly.

```markdown
## Architecture Recommendations Report

| Field | Value |
|-------|-------|
| Date | [current date] |
| Mode | Audit |
| Sections Checked | N/13 |
| Violations Found | N |

### Violations
| Module | Declared Rule | Actual Import | Severity |
|--------|--------------|---------------|----------|

### Missing Sections
- [list sections from §1 not present in architecture.md]

### Undocumented Modules
- [modules in code but not in architecture.md]

### Stale Content
- [sections describing code that no longer exists]

### Mockability Gaps
- [inter-module dependencies without mock implementations]

### Recommendations
1. [ordered list of suggested changes]
```

**Gate:** Present report to user. User responds with:
- **"Plan"** → route to `/plan-making` to implement recommendations
- **"Accept"** → no changes needed

---

## Step 5 (all paths): Validate

// turbo
1. Re-read `architecture.md` with `view_file` and verify all 15 required section headings from `architecture-rules.md §1` are present.
// turbo
2. `rg -n "## " architecture.md` (section heading audit).
3. Cross-reference the draft/report against `architecture-rules.md §1` checklist.
4. Verify all 15 required sections are present (New/Discover) or assessed (Audit).

## Rules

- This is a **Planning-mode workflow** — no source code changes allowed.
- Output: `architecture.md` (New/Discover) or Recommendations Report (Audit).
- If violations are found, **document them** — do not fix. Fixes go through `/plan-making`.
- The Architect may reference `architecture-rules.md` §7 Best Practices for recommendations, but those are advisory — the project decides.


