# Audit Rules

> Loaded by `/audit` workflow. Defines report format, finding classification, verdict criteria, and fidelity matrix.

## 1. Audit Report Format

<!-- TEMPLATE_START: audit-report -->
```markdown
## 📋 Audit Report

| Field | Value |
|-------|-------|
| **Date** | [Current date] |
| **Auditor** | Architect |
| **Scope** | post-implementation / compliance |
| **Plan Reference** | [Link to original plan, or N/A] |
| **Build Report** | [Link to builder_report.md, or N/A] |
| **Verdict** | ✅ Pass / ⚠️ Pass with notes / ❌ Fail |

### Verification Gate Results
| Check | Status |
|-------|--------|
| Formatter | ✅ / ❌ |
| Linter | ✅ / ❌ |
| Tests | ✅ / ❌ |

### Violations & Deviations
> *(If there are no violations, output a single row stating "No violations found".)*

| # | Finding | Category | Severity | File | Rule |
|---|---------|----------|----------|------|------|
| 1 | [description] | [category] | critical/high/med/low | [file:line] | [doc § section] |

### Compliant Items
> *(Positive confirmations and items that successfully passed the audit belong here, NOT in the table above.)*
- [List items that passed audit — document what's working well]
```
<!-- TEMPLATE_END -->

## 2. Finding Classification

### Categories

| Category | What it covers |
|----------|---------------|
| Plan Fidelity | Omissions, additions, deviations from the approved plan |
| Error Handling | Silent failures, missing error propagation, crash paths |
| Observability | Missing logging, unstructured logs, no instrumentation |
| Documentation | Missing doc comments, stale docs, no module-level docs |
| Testing | Missing tests, untested edge cases, no mock coverage |
| Architecture | Structural violations, dependency direction breaches |
| Code Quality | Dead code, unused imports, hardcoded values, unclear naming |
| Coding Standards | Violations of `coding-standard.md` rules |
| Security | Vulnerabilities, hardcoded secrets (verify with `make search-secrets`), injection risks |

### Severity

| Severity | Criteria | Example |
|----------|----------|---------|
| `critical` | Security vulnerability, data loss risk, production crash | SQL injection, `unwrap()` on user input |
| `high` | Incorrect behavior, missing error handling on failure path | Silent error swallowing, wrong return value |
| `medium` | Standard violation, missing test, documentation gap | No doc comment on public function, missing edge case test |
| `low` | Style, cosmetic, minor naming issue | Variable naming, import ordering |

## 3. Verdict Criteria

| Verdict | Condition |
|---------|-----------|
| ✅ **Pass** | Zero findings above `low` severity AND verification gate passes |
| ⚠️ **Pass with notes** | Only `medium` or `low` findings, no blockers, verification gate passes |
| ❌ **Fail** | Any `critical` or `high` finding, OR verification gate failure |

### Handoff Rules

- **Pass**: Proceed to Summarize phase.
- **Pass with notes**: User chooses "Plan" (remediate via `/plan-making`) or "Accept" (proceed with findings noted).
- **Documentation-only findings**: User may choose "Docs" to route directly to `/update-doc`.
- **Fail**: Must route to `/plan-making` for remediation. No direct fixes.
- **Second consecutive failure** on same scope: Escalate to user with summary of what was tried.

## 4. Fidelity Matrix

Used during post-implementation audits to verify plan adherence.

| Type | Definition | Impact |
|------|-----------|--------|
| **Omission** | Plan item not implemented | Fail — missing deliverable |
| **Addition** | Unplanned substantive change introduced | Fail unless justified and documented |
| **Minor Addition** | Pre-approved per `builder-rules.md §4` (imports, derives, formatting) | Not a finding |
| **Stale Stub** | `STUB(Phase N)` where N ≤ current phase remains (verify with `make find-stubs`) | Fail — deferred work not completed |
| **Deviation** | Implementation differs from plan | Fail unless justified and documented |
| **Justified** | Deviation with documented reasoning | Acceptable — note in findings |
| **Test Regression** | Test count decreased, test disabled, or test file deleted without plan authorization | Fail — anti-regression violation |


Scoring:
- Any unjustified Omission → ❌ Fail
- Any unjustified Addition or Deviation → ❌ Fail
- Minor Additions (as defined in `builder-rules.md §4`) are pre-approved and excluded from fidelity scoring
- Justified deviations are noted as `medium` findings with documentation
- Any unauthorized test regression (deletion, disabling, commenting out) → ❌ Fail

