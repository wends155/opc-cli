---
description: Sync rustdoc comments, spec.md, README.md, Cargo.toml description, and crate docs to actual code (Documentation Maintenance)
---

# Update-Doc Workflow

This workflow synchronizes code documentation with the actual codebase.
It detects drift in rustdoc comments, crate-level docs, and `spec.md`
behavioral contracts, then generates updates using a combination of
automated scanning, MCP code analysis, and Architect-level reasoning.

> [!IMPORTANT]
> This workflow allows **doc-comment-only edits** to source files (e.g., `///`, `//!`).
> Logic changes are **prohibited**. All other documentation changes target `spec.md`.

> [!NOTE]
> `architecture.md` is **NOT** in scope — it is owned by the `/architecture` workflow.

## Trigger

User invokes: `/update-doc`

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed rule files (e.g., `.agents/rules/...`) before starting Step 1. Do not rely on internal memory.

- Read `.agents/rules/doc-rules.md` for documentation standards.
- Read `spec.md` (if present) to understand existing behavioral contracts.
- Read `context.md` (if present) for historical decisions.
- Confirm you are operating as the **Architect** role.

## Steps

### 1. Documentation Scan (Agent Procedure)

Gather the current documentation state:

1. **Project Detection:** Read `Cargo.toml`, `package.json`, or `go.mod` with `view_file`.
2. **Toolchain Config:** Check for `rustfmt.toml`, `clippy.toml`, `.cargo/config.toml` with `view_file`.
3. **Doc Coverage (Rust):**
// turbo
   - `make find-public-api` (public items)
// turbo
   - `rg -c "\s*///" src/ --glob "*.rs"` (doc comment lines)
4. **Section Audit:** Read `spec.md` with `view_file`. Check for required section headings from `doc-rules.md §4`.
5. **Drift Detection:** If `spec.md` has `> Last verified against: <hash>`:
// turbo
   - `git rev-list --count <hash>..HEAD -- src/` (commits since verification)
6. **Package Metadata Scan:**
   - Read `Cargo.toml` with `view_file` — extract `[package.description]` (or `[workspace.package].description` if inherited).
   - Read `README.md` with `view_file` — extract current structure and content.
   - Compare `Cargo.toml` description ↔ `lib.rs //!` first line ↔ README overview.
   - Flag any drift between the three.

> [!TIP]
> Review the scan output before proceeding. If doc coverage is high and spec.md
> is up-to-date, the update may only need minor refreshes.

### 2. Code Analysis (MCP)

#### Narsil MCP

If **Narsil MCP** is available, use it to extract code-level data:

| Tool | Purpose |
|------|---------|
| `get_project_structure` | Project layout for README "Architecture" or "Project Layout" section |
| `find_symbols` | All public types, functions, traits, enums |
| `get_export_map` | Per-module exports — input for spec.md contracts |
| `get_symbol_definition` | Read signatures of undocumented public items |

If Narsil is **not available**, fall back to manual investigation:
- `make find-public-api` for public API surface
- Read key files directly

#### Sequential Thinking MCP

If **Sequential Thinking MCP** is available, use `sequentialthinking` to:

1. **Delta analysis**: Compare current docs against scan results — what's changed?
2. **Prioritize**: Which undocumented items are most important?
3. **Identify contracts**: Infer behavioral contracts from code patterns.
4. **Determine update strategy**: For each section — create, update, or leave as-is.

If Sequential Thinking is **not available**, perform this reasoning inline.

### 3. Documentation Generation (Architect LLM)

Using all data from Steps 1–2, generate or update documentation:

#### 3a. Rustdoc Comments

For each undocumented public item identified in Step 1, generate doc comments
per `doc-rules.md` §1:

- `///` doc comments with: **what**, **why**, **inputs**, **outputs**, **errors**, **examples**
- `//!` crate/module-level docs in `lib.rs` / `main.rs` / `mod.rs`

> [!CAUTION]
> Only edit doc comments (`///`, `//!`, `#[doc = ...]`). Do **not** change any logic,
> function signatures, or non-documentation code.

#### 3b. spec.md

Update or create sections per `doc-rules.md` §4:

| Section | Data Source |
|---------|------------|
| Module/Component Contracts | Narsil: `find_symbols` + `get_symbol_definition` + **LLM reasoning** |
| Data Models | Narsil: `find_symbols` (struct/enum) + **LLM reasoning** |
| State Machines | **LLM reasoning** from type analysis |
| Command/CLI Contracts | **LLM reasoning** from `main.rs` / CLI parsing |
| Integration Points | **LLM reasoning** from external deps |

#### 3c. README.md

Generate or update the full README per `doc-rules.md §7`:

1. **Preserve**: Badge lines (`[![`) and `<!-- custom:start/end -->` blocks.
2. **Generate**: All template sections from code analysis + LLM reasoning.
3. **Sync**: First paragraph must align with `Cargo.toml` description.

> [!CAUTION]
> Content inside sentinel blocks (`<!-- custom:start/end -->`) must NEVER
> be modified. Copy it verbatim into the regenerated README.

#### 3d. Cargo.toml Description

Update the `[package.description]` (or `[workspace.package.description]`) field
to match the first-line summary derived from `lib.rs //!` overview.

> [!CAUTION]
> Only the `description` field may be edited. Do NOT modify dependencies,
> features, build settings, or any other Cargo.toml content.

### 4. Record Verification Hash

After generating documentation, record the source code commit that was verified:

```powershell
$hash = git rev-parse --short HEAD
```

Write or update the metadata line in `spec.md`:

```markdown
> Last verified against: <hash>
```

This enables drift detection in future scans (see `doc-rules.md` §5).

### 5. Validate (Agent Procedure)

Re-run the documentation scan from Step 1. Confirm:
- All required spec.md sections present ✅
- Doc coverage improved from opening scan
- No drift warning (or drift is expected)

If any check fails, return to Step 3 and address the gaps.

### 6. Pause for Review

End with:

> 🛑 **Documentation Update Complete.**
> Please review the changes above. You can:
> - **Adjust** any generated documentation
> - **Add** context the LLM may have missed
> - **Remove** sections that don't apply
>
> When satisfied, reply with **"Proceed"** to commit.

**Do NOT commit until the user explicitly approves.**

### 7. Summarize (Context Compression)

> 📘 **Skill:** [`compress-context`](.gemini/skills/compress-context/SKILL.md) — compress interaction to context.md

After committing, compress the interaction per TARS protocol:

> 📝 **Context Update:**
> * **Feature:** Documentation sync for [scope]
> * **Changes:** [Summary of docs created/updated]
> * **New Constraints:** [Any new doc patterns established]
> * **Pruned:** [What intermediate analysis can be discarded]

- If `context.md` exists, append this update.
- If `context.md` does not exist, create it with this as the first entry.

## Rules

1. **Doc-only edits** — source file changes are limited to doc comments (`///`, `//!`). Package metadata edits are limited to `Cargo.toml [package.description]` and `README.md`.
2. **No logic changes** — function bodies, signatures, and imports must not be modified.
3. **No architecture.md** — `architecture.md` is owned by `/architecture`, not this workflow.
4. **Always validate** — run the script in validate mode before presenting to the user.
5. **Use MCP tools** — when Narsil or Sequential Thinking are available, prefer them for accuracy.
6. **Preserve existing content** — when updating docs, preserve user-written sections and only update stale data.
7. **Always pause** — the user must approve before committing.
8. **Record hash** — always write the verification hash to spec.md after doc generation.
9. **Sentinel preservation** — `README.md` sentinel blocks (`<!-- custom:start/end -->`) must be preserved verbatim across regenerations.
10. **Release audit integration** — This workflow may be invoked by the `rust-audit-doc-release` skill (`.gemini/skills/rust-audit-doc-release/SKILL.md`) to remediate documentation gaps discovered during a release readiness audit. This workflow does NOT trigger the release audit skill.

