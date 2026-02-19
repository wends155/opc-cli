---
description: How to perform a structured post-implementation audit (Reflect Phase)
---

# Audit Workflow

This workflow defines the standard process for auditing completed implementations.
It enforces the **Reflect** and **Summarize** phases of the TARS protocol.

## Prerequisites

- An implementation has been completed (Act Phase is done).
- The original implementation plan is available for cross-reference.
- Read `GEMINI.md` for rules and guidelines.
- Read `architecture.md` (if present) for project-specific design and toolchain.
- Read `context.md` (if present) for historical decisions.
- Confirm you are operating as the **Architect** (high-reasoning model).

## Steps

### 1. Gather Context

Before auditing, collect all relevant materials:

- **Implementation Plan**: Locate and re-read the original approved plan.
- **Changed Files**: Identify every file that was created, modified, or deleted.
- **Verification Logs**: Review any test output, lint results, or build logs from the Act phase.
- **Git Diff**: Run `git diff` or `git log` to see the exact changes made.

### 2. Compliance Audit

Systematically verify the implementation against project standards:

#### 2a. Plan Fidelity
- [ ] Every item in the approved plan was implemented
- [ ] No unapproved changes were introduced
- [ ] If deviations occurred, they are documented with justification

#### 2b. GEMINI.md Compliance
- [ ] **Error Handling**: No silent failures; errors communicate what/where/why
- [ ] **Observability**: Structured logging present for significant operations
- [ ] **Documentation**: All public functions/modules have doc comments

#### 2c. Testing & Testability
- [ ] **Unit/integration tests** exist for all new/changed logic
- [ ] **Edge cases**: Tests cover boundary conditions, empty inputs, error paths, and fringe scenarios
- [ ] **Mocks & stubs**: External dependencies are abstracted behind interfaces/traits and mocked in tests
- [ ] **Testable design**: Code avoids tight coupling to global state, filesystems, or network — dependencies are injectable
- [ ] **No crashes**: No unhandled exceptions, raw panics, or uncontrolled termination paths remain untested

#### 2d. Architecture Compliance *(if `architecture.md` exists)*
- [ ] Code follows the project's directory structure and layout conventions
- [ ] Error handling uses the project's designated strategy
- [ ] Logging uses the project's designated framework
- [ ] Testing follows the project's designated framework and conventions
- [ ] Dependencies are declared correctly
- [ ] Any new patterns are consistent with existing architecture

#### 2e. Code Quality
- [ ] Code is idiomatic for the language
- [ ] No dead code, unused imports, or commented-out blocks
- [ ] No hardcoded secrets, credentials, or environment-specific values
- [ ] Variable/function names are clear and descriptive
- [ ] Complex logic has explanatory comments

### 3. Verification Gate

Re-run the project's standard verification pipeline and confirm zero-exit:

| Check | Command | Status |
|-------|---------|--------|
| **Formatter** | *Refer to `architecture.md` § Toolchain* | ☐ Pass |
| **Linter** | *Refer to `architecture.md` § Toolchain* | ☐ Pass |
| **Tests** | *Refer to `architecture.md` § Toolchain* | ☐ Pass |

> [!IMPORTANT]
> Do NOT invent commands. Source them from `architecture.md` § Toolchain.
> If `architecture.md` is absent, inspect build/config files to determine correct commands.

### 4. Findings Report

Document the audit results using this template:

> #### Audit Report
> | Field | Value |
> |-------|-------|
> | **Date** | [Current date] |
> | **Auditor** | Architect |
> | **Plan Reference** | [Link to original plan] |
> | **Verdict** | ✅ Pass / ⚠️ Pass with notes / ❌ Fail |
>
> **Findings:**
> - [List each finding: compliant items, deviations, issues]
>
> **Required Actions:**
> - [List any items that must be fixed before the task is considered complete]
> - [If none: "No actions required."]

- If **Verdict = ❌ Fail**: Provide specific, actionable remediation steps. The Builder must fix and re-submit for re-audit.
- If **Verdict = ⚠️ Pass with notes**: Document accepted risks or deferred items.
- If **Verdict = ✅ Pass**: Proceed to the Summarize phase.

### 5. Summarize (Context Compression)

**Critical:** This step prevents context bloat per TARS protocol rules.

After a passing audit, compress the interaction into a context update:

> 📝 **Context Update:**
> * **Feature:** [Name of the feature/change]
> * **Changes:** [Summary of logic/files changed]
> * **New Constraints:** [Any new rules for future Think phases]
> * **Pruned:** [What technical debt/logs can now be ignored]

- If `context.md` exists, append this update to it.
- If `context.md` does not exist, create it with this as the first entry.

### 6. Completion

End the audit with:

> ✅ **Reflect Phase Complete.** Context has been compressed.

The task is now considered fully closed under the TARS protocol.
