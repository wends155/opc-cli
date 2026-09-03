---
description: Documentation standards for code comments and spec.md (Loaded by /update-doc)
---

# Documentation Rules

> Loaded by `/update-doc` workflow. Defines standards for rustdoc comments, module docs, spec.md ownership, and drift detection.

## 1. Doc Comment Requirements

Every public item (`pub fn`, `pub struct`, `pub enum`, `pub trait`, `pub type`) must have a doc comment covering:

| Element | Required? | Format |
|---------|-----------|--------|
| **What** | Always | First line — one-sentence summary of purpose |
| **Why** | When non-obvious | Second paragraph — rationale or design intent |
| **Inputs** | For functions | `# Arguments` section with name, type, and constraints |
| **Outputs** | For functions | `# Returns` section with type and meaning |
| **Errors** | When fallible | `# Errors` section listing each error variant and cause |
| **Panics** | When possible | `# Panics` section describing panic conditions |
| **Examples** | For public API | `# Examples` section with runnable code block |

### Example

```rust
/// Parses a configuration file into a validated `Config` struct.
///
/// Reads the file at `path`, deserializes TOML content, and validates
/// all required fields are present. Returns early on missing fields
/// rather than using defaults.
///
/// # Arguments
///
/// * `path` - Absolute path to the TOML configuration file.
///
/// # Returns
///
/// A fully validated `Config` with all fields populated.
///
/// # Errors
///
/// * [`ConfigError::NotFound`] — file does not exist at `path`.
/// * [`ConfigError::ParseError`] — file is not valid TOML.
/// * [`ConfigError::MissingField`] — a required field is absent.
///
/// # Examples
///
/// ```rust
/// let config = parse_config("/etc/app/config.toml")?;
/// assert_eq!(config.port, 8080);
/// ```
pub fn parse_config(path: &str) -> Result<Config, ConfigError> {
```

## 2. Coverage Rules

| Rule | Threshold |
|------|-----------|
| All `pub fn` items | Must have `///` doc comment |
| All `pub struct` / `pub enum` items | Must have `///` doc comment |
| All `pub trait` items | Must have `///` doc comment + method docs |
| Private helper functions | Encouraged but not required |
| Test functions | Not required |
| Re-exports (`pub use`) | Not required if original is documented |

## 3. Module-Level Docs

Every `lib.rs`, `main.rs`, and `mod.rs` must have a `//!` module-level doc comment:

```rust
//! # Module Name
//!
//! Brief description of the module's purpose and responsibilities.
//!
//! ## Overview
//!
//! What this module provides and how it fits into the larger system.
```

**Crate root** (`lib.rs`) should additionally include:
- Crate-level overview
- Feature flags (if any)
- Usage example

## 4. spec.md Ownership

The `/update-doc` workflow owns `spec.md` behavioral contracts:

| Section | Content |
|---------|---------|
| Module/Component Contracts | Public API surface, invariants, error contracts |
| Data Models | Struct/enum definitions with field meanings |
| State Machines | State transitions and valid sequences |
| Command/CLI Contracts | CLI arguments, flags, expected behavior |
| Integration Points | External APIs, databases, message queues |

> [!NOTE]
> `architecture.md` is owned by the `/architecture` workflow — NOT by `/update-doc`.
> For `spec.md` template, BDD format conventions, and section requirements, see `spec-rules.md`.

## 5. Drift Detection

spec.md must contain a metadata line recording the source code commit it was last verified against:

```markdown
> Last verified against: abc1234
```

- The hash is the **source code** commit (`git rev-parse --short HEAD`) at the time of verification.
- `Scan-ProjectDocs.ps1` compares this hash against HEAD to detect drift.
- If source files changed since the recorded hash, the script emits an advisory warning.
- The hash does NOT refer to the spec.md commit (avoids chicken-and-egg).

## 6. Style

| Convention | Standard |
|------------|----------|
| **Tense** | Use present tense ("Returns the value") not future ("Will return") |
| **Voice** | Use third person ("Parses the input") not imperative ("Parse the input") |
| **Cross-references** | Use `[`backtick links`]` to reference other items: ``[`Config`]``, ``[`parse_config`]`` |
| **Line length** | Soft wrap at 100 characters for readability |
| **Markdown in docs** | Use headers (`#`), lists, code blocks — rustdoc renders full Markdown |
| **Deprecation** | Use `#[deprecated(since = "x.y.z", note = "Use X instead")]` with doc explanation |

## 7. Package Documentation Ownership

The `/update-doc` workflow owns all package-level documentation surfaces:

| Artifact | Scope |
|----------|-------|
| `README.md` (full file) | All sections: overview, install, usage, features, API surface, contributing, license |
| `Cargo.toml` `description` | Single `description` field only — no other `[package]` fields |
| `lib.rs` / `main.rs` `//!` | Crate-level overview (already in scope via §3) |

**Synchronization Rule:** The first-line summary in `Cargo.toml [package.description]` and the first paragraph of the `//!` crate doc in `lib.rs` must be semantically equivalent. The README overview section expands on this summary with full context.

### README Section Template

```markdown
# Crate Name

> One-line description (synced with Cargo.toml)

## Overview
## Installation
## Usage / Quick Start
## Features / Feature Flags
## API Surface (optional — for libraries)
## Architecture (optional — link to architecture.md)
## Contributing (optional)
## License

<!-- custom:start -->
(user-authored content preserved across regenerations)
<!-- custom:end -->
```

### Preservation Rules

- Content inside `<!-- custom:start -->` / `<!-- custom:end -->` sentinel blocks is never overwritten.
- Badge lines at the top of the file (lines matching `[![`) are preserved in-place.
- If no sentinel blocks exist, the workflow generates the full README from scratch.

### Workspace Inheritance

If `Cargo.toml` contains `description.workspace = true`, the workflow must edit the workspace root `Cargo.toml` (`[workspace.package].description`) instead of the crate-level one.
