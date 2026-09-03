# Phase Rules

> Loaded by `/plan-making` (multi-phase plans), `/build` (stub registry), and `/audit` (phase gate).
> Defines conventions for multi-phase project planning, stub tracking, and inter-phase fidelity.

## 1. Phase Scoping

Each phase should focus on **one primary concern**. Phases are numbered sequentially and named by their focus.

### Typical Phase Progression

| Phase | Focus | Typical Scope |
|:---:|:---|:---|
| **1** | Foundation | Workspace scaffold, error types, config system, tracing, DB migrations, Docker compose, CI, core domain types |
| **2** | Core Feature | Primary business logic — the main thing the application does |
| **3+** | Extensions | Additional features, external integrations, secondary workflows |
| **N** | Hardening | Replace stubs, address tech debt, performance tuning, security review |

### Phase 1 Foundation Checklist

When Phase 1 is a foundation phase, it should deliver:

- [ ] Workspace structure (`Cargo.toml` / workspace layout)
- [ ] Error handling foundation (error types, `Result` patterns)
- [ ] Configuration system (`.env` loading, config struct per `coding-standard.md §4.10`)
- [ ] Observability foundation (tracing setup, structured logging)
- [ ] Database setup (migrations, connection pooling) — if applicable
- [ ] Docker compose for local dev services — if applicable
- [ ] Core domain types (newtypes, shared types module)
- [ ] CI pipeline skeleton — if applicable
- [ ] `architecture.md` fully documented

> [!TIP]
> A strong Phase 1 foundation means Phase 2+ can focus on features, not plumbing.
> If Phase 2 discovers the error handling strategy is wrong, you're rewriting everything.

## 2. Phase Manifest

Every multi-phase plan must include a **Phase Manifest** section. This is the inter-phase contract — it tracks what was delivered, what was deferred, and what debt was incurred.

### Format

```markdown
## Phase Manifest

### Delivered
- ✅ Config system with .env loading
- ✅ Error handling foundation (thiserror)
- ✅ Database migrations (sqlx)

### Deferred (Stub Registry)
| Stub | File | Scheduled | Contract |
|------|------|-----------|----------|
| `MockEmailSender` | src/infra/email.rs | Phase 3 | trait EmailSender — send(&self, to, subject, body) -> Result |
| `NoOpRateLimiter` | src/api/middleware.rs | Phase 4 | trait RateLimiter — check(&self, ip: IpAddr) -> Result<(), RateLimitError> |

### Tech Debt
| Item | Class | Phase | Notes |
|------|-------|-------|-------|
| Hardcoded timeout (30s) | Opportunistic | next available | Should come from config |
| No retry on DB connection | Scheduled | Phase 3 | Add exponential backoff |
```

### Rules

1. **Delivered**: List all features/components shipped in this phase.
2. **Deferred (Stub Registry)**: Every stub created must be registered with:
   - **Stub**: The type or function name
   - **File**: Where it lives
   - **Scheduled**: Which phase should replace it
   - **Contract**: The trait/interface the real implementation must satisfy
3. **Tech Debt**: Every known shortcoming must be classified (see §4).
4. The Phase Manifest from the **previous phase** is input to the **next phase's** plan.
5. Stubs scheduled for the current phase become **mandatory plan steps**.

## 3. STUB Marker Convention

When deferring work to a later phase, use structured `STUB` markers instead of `todo!()`.

### Format

```rust
// STUB(Phase 3): Replace with real email sender via SendGrid API
// Contract: trait EmailSender — send(&self, to: &str, subject: &str, body: &str) -> Result<(), EmailError>
pub struct MockEmailSender;

impl EmailSender for MockEmailSender {
    fn send(&self, _to: &str, _subject: &str, _body: &str) -> Result<(), EmailError> {
        tracing::warn!("STUB: email not actually sent");
        Ok(())
    }
}
```

### Rules

1. **Format**: `// STUB(Phase N): description` — the phase number and description are mandatory.
2. **Contract comment**: Every stub MUST have a contract line specifying the trait/interface the real implementation must satisfy.
3. **Functional code**: Stubs must be functional — return `Ok(())`, log a warning, or provide a no-op implementation. Never use `todo!()`, `unimplemented!()`, or `panic!()`.
4. **Auditable**: All stubs are discoverable via `rg "STUB\(Phase"`. This is used by `/audit` for stale stub detection.
5. **Registered**: Every stub must also appear in the Phase Manifest's Deferred table.
6. **`todo!()` remains prohibited** — per `coding-standard.md §10`. STUB markers are the sanctioned replacement for multi-phase deferred work.

> [!CAUTION]
> A stub without a contract comment is a **compliance violation**. The contract
> is what ensures the replacement implementation satisfies the same interface.

## 4. Tech Debt Classification

All tech debt must be classified by when it should be addressed:

| Class | When to Address | Example |
|:---|:---|:---|
| **Blocking** | Must fix before next phase starts | Broken abstraction that prevents feature work |
| **Scheduled** | Assigned to a specific future phase | "Replace MockEmailSender in Phase 3" |
| **Opportunistic** | Fix when you're "in the neighborhood" | Minor naming cleanup, extra test case |
| **Accepted** | Won't fix — documented design choice | "We chose X over Y for reason Z" |

### Rules

1. **Blocking** debt halts phase progression — the phase gate (§5) will catch it.
2. **Scheduled** debt must have a phase number. It becomes mandatory when that phase starts.
3. **Opportunistic** debt may be addressed during any phase if the Builder is already modifying the relevant file. It does NOT justify scope expansion — the change must be minor (per `builder-rules.md §4.2`).
4. **Accepted** debt is a conscious design decision. Document the rationale so future phases don't re-litigate.

## 5. Phase Gate

Before starting Phase N+1, the Architect must perform a **phase gate review**. This can be part of the `/audit` at the end of Phase N, or a standalone check before `/plan-making` for Phase N+1.

### Phase Gate Checklist

- [ ] All plan items from Phase N are `[x]` in `task.md`
- [ ] Phase N's audit verdict is ✅ Pass or ⚠️ Pass with notes (accepted)
- [ ] No **Blocking** tech debt remains from Phase N
- [ ] All **Scheduled(Phase N)** stubs were replaced — verify with `rg "STUB\(Phase N\)"`
- [ ] Prior phase test suites still pass (`ALL` exits 0)
- [ ] Phase Manifest from Phase N is recorded in `context.md`
- [ ] Stubs scheduled for Phase N+1 are identified and ready for plan inclusion

### Gate Outcome

| Result | Action |
|:---|:---|
| All checks pass | Proceed to `/plan-making` for Phase N+1 |
| Blocking debt remains | Address before planning next phase |
| Stale stubs found | Must be remediated or rescheduled with justification |
| Prior tests fail | Investigate regression before proceeding |

## 6. Inter-Phase Fidelity

How to ensure contracts survive between phases:

### Trait Signature Freeze

Once a trait is delivered in Phase N and other code depends on it:
- The trait signature is **frozen** — future phases must not change it without a migration plan.
- Adding new methods to the trait requires a default implementation (backward compatible).
- Changing existing method signatures requires updating ALL callers in the same phase.

### Test Suite Continuity

- Test suites from ALL prior phases must pass at every checkpoint.
- A Phase 3 change that breaks a Phase 1 test is a **regression**, not a "Phase 1 concern."
- The Builder's `ALL` verification includes all prior phase tests automatically (they're in the same codebase).

### Stub Contract Binding

When replacing a stub in Phase N:
- The replacement MUST satisfy the contract specified in the STUB marker comment.
- The replacement MUST pass all existing tests that exercised the stub.
- If the contract needs to change, the Architect must update all callers in the same plan.

> [!IMPORTANT]
> Stubs are contracts, not placeholders. Replacing `MockEmailSender` with
> `SendGridEmailSender` means satisfying **exactly** the `EmailSender` trait —
> same method signature, same error type, same guarantees.

---

> **Loaded by:** `/plan-making` (multi-phase), `/build` (stub check), `/audit` (phase gate)
> **Compliance:** Phase gate verified during `/audit`; stale stubs are audit findings per `audit-rules.md §4`
