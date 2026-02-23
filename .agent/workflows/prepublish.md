---
description: Pre-publication QA/QC gate for crates.io releases (Prepublish Phase)
---

# Prepublish Workflow

This workflow defines the standard process for verifying a crate is ready for `crates.io` publication.
It enforces documentation correctness, version synchronization, security scanning, and a simulated publish dry-run.

## Prerequisites

- Read `GEMINI.md` for rules and guidelines.
- Read `architecture.md` (if present) for project-specific design and toolchain.
- Read `coding_standard.md` (if present) for language-specific coding standards.
- Read `context.md` (if present) for historical decisions.
- Confirm you are operating as the **Architect** (high-reasoning model).
- Identify the target crate directory (e.g., `opc-da-client/`).

## Steps

### 1. Context Initialization

Load and parse all publication-relevant files:

- `README.md` — User-facing documentation and install instructions.
- `architecture.md` — Internal design documentation.
- `spec.md` — API specification.
- `CHANGELOG.md` — Release history.
- `Cargo.toml` — Crate manifest and metadata.
- `src/lib.rs` — Top-level rustdoc comments and examples.

### 2. Version Synchronization Check

Extract the **canonical version** from `Cargo.toml` `[package].version` and sweep the entire crate for alignment:

| Location | What to Check |
|----------|---------------|
| `README.md` | Install instructions (`opc-da-client = "X.Y.Z"`), badges, dependency snippets |
| `Cargo.toml` | `[package].version`, `[package.metadata.docs.rs]` settings |
| `src/**/*.rs` | Rustdoc comments, module-level docs, `lib.rs` header examples |
| `CHANGELOG.md` | Top entry heading must match the canonical version |
| Badges/Shields | Any `docs.rs` or `crates.io` version badges |

> [!IMPORTANT]
> Flag **any** stale or mismatched version string. A single mismatch is a blocking finding.

### 3. Documentation Consistency Check

Cross-reference documentation against the current API surface:

- [ ] `README.md` examples use current struct/function names and signatures
- [ ] `architecture.md` reflects current module layout and error handling strategy
- [ ] `spec.md` function signatures match the actual code
- [ ] `lib.rs` top-level doc examples compile under the current API
- [ ] No references to removed, renamed, or deprecated types/methods
- [ ] Error handling documentation reflects the current strategy (e.g., `OpcResult` vs `anyhow::Result`)

### 4. Manifest (Cargo.toml) QC

Validate the crate manifest for registry readiness:

- [ ] `description` is present and descriptive
- [ ] `license` is set (e.g., `MIT`, `Apache-2.0`)
- [ ] `repository` points to the correct GitHub URL
- [ ] `keywords` are populated (max 5, relevant to the domain)
- [ ] `categories` are set appropriately
- [ ] `docs.rs` metadata target is correct (e.g., `default-target = "x86_64-pc-windows-msvc"`)
- [ ] `exclude` blocks suppress non-registry artifacts (`.winmd`, test fixtures, spec files, scripts)
- [ ] No unnecessary or unused dependencies in `[dependencies]`
- [ ] All dependency versions are pinned or ranged appropriately

### 5. Security Scan

Run a security sweep before publishing to the public registry:

- Use Narsil `scan_security` to check for code-level vulnerabilities.
- Use Narsil `check_dependencies` to verify no known CVEs exist in the dependency tree.

> [!CAUTION]
> Any **critical** or **high** severity finding is an automatic **Blocked** status. Do NOT proceed.

### 6. Verification Gate

Execute the workspace's standard verification pipeline:

```powershell
# From the repository root
.\scripts\verify.ps1
```

This runs:
- `cargo fmt --check` — Formatting compliance
- `cargo clippy --all-features` — Lint compliance
- `cargo test` — Unit and integration tests
- `cargo test --doc` — Doc-test compilation and execution

> [!IMPORTANT]
> All checks must return **zero-exit**. Any failure is a blocking finding.
> Do NOT invent commands. Source them from `architecture.md` § Toolchain or the existing `verify.ps1`.

### 7. Simulated Publication Gate

Run a dry-run publish to validate the package without uploading:

```powershell
cargo publish --dry-run
```

Verify:
- [ ] No unexpected files leak into the published crate
- [ ] Compression and file boundaries are structurally valid
- [ ] No warnings about missing fields or metadata

### 8. Prepublish Report

Produce a `prepublish_report.md` artifact using this exact structure:

---

> #### Prepublish Report
> | Field | Value |
> |-------|-------|
> | **Date** | [Current date] |
> | **Auditor** | Architect |
> | **Crate** | [Crate name and version] |
> | **Verdict** | ✅ Clear to Publish / 🛑 Blocked: Remediation Required |
>
> #### Audit Results
> | Gate | Status | Notes |
> |------|--------|-------|
> | Version Sync | ✅ / ❌ | [Details] |
> | Docs Consistency | ✅ / ❌ | [Details] |
> | Cargo Manifest | ✅ / ❌ | [Details] |
> | Security Scan | ✅ / ❌ | [Details] |
> | Verification Gate | ✅ / ❌ | [Details] |
> | Dry-Run | ✅ / ❌ | [Details] |
>
> #### Remaining Action Items
> | What | Why |
> |------|-----|
> | [Action needed] | [Reason it matters] |
> | *e.g., Push git tag v0.2.0* | *Align GitHub releases with crates.io* |
>
> #### Recommendations
> - [Proactive suggestions observed during the sweep]
> - *e.g., "Consider adding an example for browse_tags"*
> - *e.g., "Dependency X is trailing by 2 minor versions"*

---

**Verdict rules:**
- **"Clear to Publish"** — ONLY if 100% of all gates pass autonomously. Zero exceptions.
- **"Blocked: Remediation Required"** — If **any single gate** fails, regardless of severity.

### 9. Completion

Present the report to the user via `notify_user`.

- If **Clear to Publish**:
  > ✅ **Prepublish QA Complete.** Crate is ready for `cargo publish`.

- If **Blocked**:
  > 🛑 **Blocked.** Remediation required before publishing. See the report for details.
