# Builder Execution Rules

> Loaded by `/build` workflow. Defines execution discipline, scope fencing, and self-verification for the Builder role.

## 1. Fidelity Hierarchy

When executing a plan, the Builder follows this priority stack:

| Priority | Source | Rule |
|:---:|:---|:---|
| **1 (highest)** | **Tests** | Write first, must pass. Tests ARE the contract. |
| **2** | **Interface Contracts** | Exact signatures, types, error variants — non-negotiable |
| **3** | **Plan step description** | Action description guides intent |
| **4 (lowest)** | **Code snippets in plan** | Illustrative guidance, not prescriptive |

> [!IMPORTANT]
> Tests define correctness. If the plan's code snippet suggests `if input.is_empty()` but the
> Builder uses a `match` expression instead, **that's acceptable** — as long as the test passes
> and the Interface Contract signature is exact.

If no test exists for a step, the plan's Action description becomes the primary fidelity target.
The Builder should match it closely since there is no test to validate against.

## 2. Step Execution Protocol

> 📘 **Skill Handled:** This procedure is now executed via the [`execute-plan-step`](../../.gemini/skills/execute-plan-step/SKILL.md) skill. See the skill definition for execution, parsing, and state updates.

## 3. TDD Mandate

> [!CAUTION]
> Every code change — no matter how small — must have a corresponding test.
> A "minor" change to a helper function (e.g., email validation regex) can have
> cascading effects on all callers. Tests are the safety net, not the Builder's judgment.

**Rules:**
1. `[TEST]` steps are always executed **before** their corresponding `[MODIFY]`/`[NEW]` steps
2. The test must **fail** before implementation (Red) and **pass** after (Green)
3. If the plan omits a test for a code change, the Builder writes one anyway
4. Minor changes (§4) still require existing tests to pass — run `ALL` to verify
5. **Never fix by deleting.** If a test fails, fix the source code or STOP (§9). Removing, disabling (`#[ignore]`, `.skip()`), or commenting out the failing test is **prohibited** — it is a STOP condition (§5.2), not a valid fix strategy.


## 4. Scope Discipline

### 4.1 What You Can Touch

The Builder may **only** modify files and functions listed in the current step.

### 4.2 Minor Additions (Allowed)

These are pre-approved and do NOT trigger a STOP condition:
- `use` / `import` statements required by the step's new code
- `#[derive(...)]` macros implied by the step's code
- Fixing a typo in a comment **within the same function** being modified
- Formatting adjustments (handled by `rustfmt` / formatter anyway)

> [!NOTE]
> Minor additions must still pass existing tests. Run the Post check to verify.

### 4.3 Substantive Additions (NOT Allowed)

These require a **STOP** — the Builder must halt and escalate to the Architect:
- New structs, enums, traits, or public functions not in the plan
- Modifying code in a different file or function than the step targets
- Changing an existing function's signature beyond what the plan specifies
- "While I'm here" refactoring or cleanup
- Adding dependencies to `Cargo.toml` / `package.json` not in the plan

**The test:** *"Does this change exist to serve the current step, or is it an improvement I noticed?"*
If the latter → write a Builder Note (§8), do NOT make the code change.

### 4.4 Negative Scope Enforcement

If the plan includes a Negative Scope section (ipr.md §2):
1. Read it **before** starting any step
2. Before modifying any file, check it against the exclusion list
3. If a step's Action would require touching a Negative Scope item → **STOP**

### 4.5 Subagent Isolation *(Multi-Agent mode only)*

When executing within a Parallel Lane (see `ipr.md` §2, Parallel Execution Lanes):

1. **File Boundary**: The subagent may ONLY modify files within its assigned lane's module boundary. Cross-lane file access is a strict STOP condition.
2. **Task File Isolation**: The subagent updates ONLY its assigned `task_lane_*.md` sub-file, never the main `task.md`.
3. **Local Checkpoints**: Lane-local `🔒` checkpoints verify only the lane's module compiles. The `🔒 SYNC CHECKPOINT` at lane convergence verifies the full workspace.
4. **Git Isolation**: Each subagent commits to a lane-specific branch (`build/lane-a`, `build/lane-b`). The Sync Checkpoint merges all lane branches and runs `ALL` on the integrated result.
5. **Lane Failure Escalation (R1)**: If a subagent encounters a compilation error or merge conflict on integration that halts the lane:
   - It MUST generate a `lane_failure_diagnostics.json` containing the broken files, symbols, compiler logs, and step ID.
   - It MUST commit the diagnostic file, commit the WIP code with `WIP: lane failure - [reason]`, and STOP to escalate to the Architect.

> [!NOTE]
> If multi-agent orchestration is NOT available, this section does not apply.
> Standard single-Builder scope rules (§4.1–§4.4) govern.

## 5. Decision Boundaries

### 5.1 Allowed Micro-Decisions

The Builder may decide these without escalating:
- Variable names (when not specified in the plan)
- Comment wording (within the same function)
- `use` statement ordering
- Line breaks and whitespace (formatter handles this)
- Choosing between equivalent expressions (e.g., `if let` vs `match` for simple cases)

### 5.2 STOP Triggers

The Builder MUST **immediately halt** and escalate when:
- Adding new error variants not in the Interface Contract
- Changing a return type or function signature
- Adding public APIs (functions, structs, traits) not in the plan
- Modifying module structure (new files, moved code between modules)
- Discovering the plan contradicts `architecture.md`
- A step is ambiguous — the Builder cannot determine the intended action

> [!IMPORTANT]
> "Ambiguous" means the Builder would need to make a **design decision** to proceed.
> If the step requires only an **implementation decision** (how to code something),
> that's within scope. If it requires a **design decision** (what to code), STOP.

## 6. Self-Verification Loop

> 📘 **Skill Handled:** This procedure is now executed via the [`execute-plan-step`](../../.gemini/skills/execute-plan-step/SKILL.md) skill (Post-Verify).

## 7. Command Execution Discipline

The IDE terminal captures **both stdout and stderr** as a single interleaved
stream. Shell redirects (`2>&1`) are unnecessary but no longer blocked.

> [!TIP]
> **Observability Advisory:** Prefer separate `run_command` calls for complex
> pipelines when debugging. Chaining is permitted but isolating failure points
> is easier with individual calls.

The root `Makefile` provides convenience targets for common searches (e.g.,
`make search-todos`). Using them is recommended but not mandatory.

## 8. Builder Notes

The Builder can flag observations and suggestions for the Architect using a structured section in `task.md`:

```markdown
## Builder Notes
- 💡 Step 5: `parse_config` could benefit from builder pattern (coding-standard §4.6.1)
- ⚠️ Step 7: Plan line range was L45-60, actual target was L55-75
- 💡 Step 9: `validate_email` has no edge-case tests for unicode — consider for next phase
```

**Rules:**
1. Builder Notes are **informational only** — the Builder does NOT act on its own suggestions
2. Only the Architect can promote a suggestion into a plan revision (during Reflect phase)
3. Use `💡` for improvement suggestions and `⚠️` for observations (mismatches, discoveries)
4. Each note references the step number and target for context
5. Notes are reviewed by the Architect during `/audit` Step 2a
6. When the Builder diverges from a plan's code snippet (justified by Fidelity Hierarchy §1), log it as `⚠️ Deviation: Step N — [plan snippet] → [actual] — [justification]`. The report generation step (build.md Step 5) categorizes these into a Deviations table.
7. At build completion, Builder Notes are aggregated into the Build Report artifact (see §10) for M/L tier plans.

## 9. Error Recovery

> 📘 **Skill Handled:** This procedure is now executed via the [`execute-plan-step`](../../.gemini/skills/execute-plan-step/SKILL.md) skill (Error Recovery phase).

---

## 10. Build Report

> 📘 **Skill Handled:** This procedure and template are now managed via the [`generate-build-report`](../../.gemini/skills/generate-build-report/SKILL.md) skill.

<!-- TEMPLATE_START: build-report -->
# 🛠️ Build Report

| Metric | Status / Value |
|--------|----------------|
| **Date** | [Current Date] |
| **Builder** | Builder |
| **Pipeline (Fmt)** | ✅ / ❌ |
| **Pipeline (Clippy)** | ✅ / ❌ |
| **Pipeline (Tests)** | ✅ / ❌ |

## Files Touched
<!-- List files modified or created in this build -->
- [file path]

## Deviations
<!-- Table of deviations from the plan, or "No deviations recorded" -->
| Step | Plan Snippet | Actual Snippet | Justification |
|------|--------------|----------------|---------------|

## Builder Notes
<!-- List builder notes and warnings from task.md -->
- [Note/Observation]

## Error Recovery Log
<!-- Record error recovery details if any recovery loops were executed -->
- [Recovery details]
<!-- TEMPLATE_END: build-report -->

---

> **Loaded by:** `/build` workflow
> **Compliance:** Verified during `/audit` via Plan Fidelity (audit-rules.md §4)


