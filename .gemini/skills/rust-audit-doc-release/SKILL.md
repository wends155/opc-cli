---
name: rust-audit-doc-release
description: >
  On-demand procedural audit for Rust crate release readiness: crates.io
  manifest fields, rustdoc API coverage, CHANGELOG.md formatting, README
  badges, license files, package content hygiene, and cargo publish dry-run.
---

# Rust Audit Doc Release Procedure

## When to Use
Load this skill on-demand when preparing a Rust crate for release on [crates.io](https://crates.io) or tagging a release on GitHub. 

This skill runs standalone to perform a comprehensive 8-step release readiness audit. If documentation gaps are discovered during the audit, this skill can invoke or recommend the `/update-doc` workflow for doc comment remediation.

## Prerequisites
- Read `.agents/rules/doc-rules.md` for documentation standards (§1 doc comments, §7 README template).
- Read `Cargo.toml` in the repository root for package metadata.
- Read `context.md` (if present) for project release context.

## Constraints
- **Doc-Only Edits**: Only documentation comments (`///`, `//!`), package description in `Cargo.toml`, `README.md`, `CHANGELOG.md`, `LICENSE-*`, and `spec.md` may be updated. Code logic must NOT be modified.
- **Tiered Enforcement**: Hard Blockers stop execution and must be fixed before release. Advisories are logged in the summary report as warnings.
- **Interactive Licensing**: If `Cargo.toml` lacks a license or root license files are missing, the skill MUST interactively prompt the user to choose/confirm the license before scaffolding.
- **Strict Package Hygiene**: Internal LLM/agent files (`.agents/`, `.gemini/`, `.cursor/`, `.vscode/`), logs (`transcript.jsonl`), `scratch/`, and sensitive files (`.env*`) MUST NOT be packed into `.crate`. Auto-generate `exclude` rules if found.
- **Approval Gate**: Pause and request explicit user confirmation before committing any changes.

---

## Tiered Enforcement Reference

| Category | Finding Type | Severity | Action |
|:---|:---|:---|:---|
| **Manifest** | Missing mandatory `Cargo.toml` fields (`description`, `license`/`license-file`, `repository`, `readme`) | 🔴 **Hard Blocker** | Execution STOP. Requires resolution. |
| **Compiler** | `cargo doc --no-deps` warnings or broken intradoc links (e.g. ``[`MissingType`]``) | 🔴 **Hard Blocker** | Execution STOP. Fix doc links. |
| **Publish** | `cargo publish --dry-run` failure | 🔴 **Hard Blocker** | Execution STOP. Fix manifest/deps. |
| **Leak Safety**| Presence of local `file:///` paths in `README.md` or `CHANGELOG.md` | 🔴 **Hard Blocker** | Execution STOP. Sanitize paths. |
| **Hygiene** | LLM/agent or sensitive files in `cargo package --list` | 🔴 **Hard Blocker** | Execution STOP. Auto-add `exclude`. |
| **Metadata** | Missing optional fields (`categories`, `keywords`, `rust-version`/MSRV) | 🟡 **Advisory** | Log in report as warning. |
| **Doc Style** | Secondary `pub fn` missing `# Examples` section (has summary `///`) | 🟡 **Advisory** | Log in report as recommendation. |

---

## Banlist Patterns (Package Hygiene)

The following paths MUST NOT be included in published `.crate` tarballs:
```
.agents/*
.gemini/*
.cursor/*
.vscode/*
scratch/*
transcript.jsonl
*.log
.env*
*.key
*.pem
```

---

## Procedure

### Step 1: Manifest & Environment Scan
1. Read `Cargo.toml` with `view_file`.
2. Check mandatory fields: `name`, `version`, `edition`, `description` (≥10 chars), `license` or `license-file`, `repository`, `readme`.
3. Check optional fields: `keywords` (max 5), `categories` (max 5), `rust-version` (MSRV), `documentation`, `homepage`.
4. Classify missing fields as Hard Blockers or Advisories per the Enforcement Table.

### Step 2: Package Content Hygiene Scan
1. Run `cargo package --list` using `run_command`.
2. Audit output against the Banlist Patterns.
3. If any banned file/directory appears in the packaged files:
   - Flag as 🔴 **Hard Blocker**.
   - Auto-generate or update the `[package.exclude]` array in `Cargo.toml`:
     ```toml
     [package]
     exclude = [
         "/.agents",
         "/.gemini",
         "/.cursor",
         "/.vscode",
         "/scratch",
         "transcript.jsonl",
         "*.log",
         ".env*",
     ]
     ```

### Step 3: License & Root Docs Verification
1. Inspect `Cargo.toml` `license` field (e.g. `MIT OR Apache-2.0`).
2. Verify root license files exist (`LICENSE-MIT`, `LICENSE-APACHE`, or `LICENSE`).
3. If `license` is missing in `Cargo.toml` OR root license files are missing:
   - Interactively prompt the user for target license model (`MIT OR Apache-2.0`, `MIT`, `Apache-2.0`, or custom).
   - Auto-scaffold standard license boilerplate populated with current year and copyright holder from `Cargo.toml`.
   - Update `Cargo.toml` `license` field.

### Step 4: API Rustdoc & Intradoc Link Scan
1. Run `cargo doc --no-deps --all-features` using `run_command`.
2. Audit output for rustdoc warnings or broken intradoc link errors.
3. Scan public items (`pub fn`, `pub struct`, `pub enum`, `pub trait`) for doc comments:
   - Missing `///` doc comment on public item → 🔴 **Hard Blocker**.
   - Broken intradoc link (e.g. ``[`MissingType`]``) → 🔴 **Hard Blocker**.
   - Missing `# Examples` section on secondary function (with `///` present) → 🟡 **Advisory**.

### Step 5: Local Path Leak Sweep
1. Search `README.md`, `CHANGELOG.md`, and `spec.md` for local `file:///` URLs.
2. If local `file:///` links are found:
   - Flag as 🔴 **Hard Blocker**.
   - Replace with public GitHub or docs.rs relative/HTTP URLs.

### Step 6: Git Commit Delta & Changelog Draft
1. Determine last release tag: `git describe --tags --abbrev=0` (or HEAD if no tags exist).
2. Parse commit delta: `git log <last-tag>..HEAD --oneline`.
3. Categorize commits by prefix into *Keep a Changelog* format:
   - `feat:` → `Added`
   - `fix:` → `Fixed`
   - `perf:` / `refactor:` → `Changed`
   - `breaking:` → `Removed` / `Changed`
   - `docs:` → `Changed` (docs)
4. Draft new `[vX.Y.Z] - YYYY-MM-DD` section and append to `CHANGELOG.md`, preserving past entries verbatim.

### Step 7: Toolchain Dry-Run Verification
1. Run `cargo publish --dry-run --allow-dirty` using `run_command`.
2. Verify zero errors returned.
3. If exit code ≠ 0 → 🔴 **Hard Blocker**. Log compiler/manifest error.

### Step 8: Report Generation & Review Gate
1. Write the `release_audit_report.md` artifact detailing all findings, hard blockers, advisories, and proposed fixes.
2. **Remediation Handoff**: If public API doc gaps were discovered in Step 4, recommend running `/update-doc` to generate doc comments before release.
3. End with approval gate:
   > 🛑 **Release Audit Complete.** Review findings in `release_audit_report.md`. Reply with **Proceed** to apply fixes & commit.

---

## Report Template (`release_audit_report.md`)

```markdown
# Crate Release Audit Report: <crate-name> v<version>

**Date:** YYYY-MM-DD  
**Status:** <PASSED | HARD BLOCKERS FOUND>  
**Target Version:** vX.Y.Z  

## Executive Summary
<Brief overview of release readiness>

## 🔴 Hard Blockers (Must Fix Before Release)
| ID | Category | Finding | Action Required |
|---|---|---|---|
| H1 | <Category> | <Description> | <Fix> |

## 🟡 Advisories & Recommendations
| ID | Category | Finding | Recommendation |
|---|---|---|---|
| A1 | <Category> | <Description> | <Suggestion> |

## 📦 Package Content Hygiene Audit
- **Total packaged files:** N
- **Banlist violations:** <None | List of files>
- **Status:** <CLEAN | EXCLUDE ARRAY UPDATED>

## 📝 Draft Changelog Section
```markdown
## [vX.Y.Z] - YYYY-MM-DD

### Added
- <feature 1>

### Fixed
- <fix 1>
```

## 🛠️ Toolchain Dry-Run Status
- `cargo doc`: <PASS | FAIL>
- `cargo publish --dry-run`: <PASS | FAIL>
```
