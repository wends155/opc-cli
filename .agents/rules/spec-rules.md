# Spec Rules

> Loaded by `/spec` workflow. Defines the required template, BDD conventions, and format standards for `spec.md`.

## 1. Required Sections

Every project's `spec.md` must contain applicable sections from this list:

| # | Section | Primary Format | When Required |
|---|---------|---------------|---------------|
| 1 | Module/Component Contracts | BDD scenarios + API table | Always (one per public module) |
| 2 | Data Models | Field table + BDD for complex validation | When persistent types or shared DTOs exist |
| 3 | State Machines | Mermaid `stateDiagram-v2` + transition table | When stateful components exist |
| 4 | Command/CLI Contracts | Arg table + BDD for behavior | When CLI interface exists |
| 5 | Integration Points | Endpoint table + BDD for edge cases | When external APIs or inter-service boundaries exist |

> [!TIP]
> You don't need to spec everything upfront. Start by specifying modules you're actively working on.
> Prioritize public API boundaries and state machines over internal helpers.

## 2. BDD Scenario Conventions

`spec.md` uses a **hybrid format**: BDD (GIVEN/WHEN/THEN) as the primary notation for behavioral contracts, with tables, diagrams, and prose where BDD is awkward.

### When to Use BDD

- **Use BDD** for: behavioral contracts, error handling, edge cases, business rules, validation logic, security requirements
- **Use tables** for: API signatures, data model fields, CLI arguments, endpoint inventories
- **Use diagrams** for: state machines, data flow, transition sequences
- **Use prose** for: high-level summaries, architectural rationale (one-liners only)

### Scenario Format

```
### [Module Name] — [Behavior Area]

[HAPPY] Successful operation
GIVEN a valid patient record
WHEN the record is saved
THEN the record is persisted with a generated UUID
AND the `created_at` timestamp is set to the current time

[ERROR] Missing required field
GIVEN a patient record without a `name` field
WHEN the record is validated
THEN validation fails with `ValidationError::MissingField("name")`

[EDGE] Concurrent modification
GIVEN a record being edited by two sessions
WHEN both sessions save simultaneously
THEN the server-authoritative version wins
AND the losing version is archived in `sync_conflicts`

[SECURITY] Rate-limited login
GIVEN a user with 3 consecutive failed login attempts
WHEN they attempt a 4th login
THEN the account is locked for 5 minutes
AND a security event is logged
```

### Scenario Tags

| Tag | Meaning |
|-----|---------|
| `[HAPPY]` | Expected success path |
| `[ERROR]` | Error conditions and failure modes |
| `[EDGE]` | Boundary conditions, race conditions, corner cases |
| `[SECURITY]` | Security-relevant behavior (auth, rate-limiting, input sanitization) |

### Rules

1. Each scenario is standalone — it must be understandable without reading others
2. Scenarios map to test cases — if it can't be tested, it's not a spec
3. Use concrete values in examples, not abstract placeholders ("Dr. Smith", not "\<user\>")
4. Reference error types by name (`ValidationError::MissingField`, not "an error")
5. One GIVEN clause per scenario — split complex preconditions into separate scenarios

## 3. Module Contract Template

For each public module, use this template:

```markdown
## [Module Name]

> One-line description of the module's purpose.

### Public API

| Function | Signature | Returns | Errors |
|----------|-----------|---------|--------|
| `create_patient` | `(&self, input: CreatePatient) -> Result<Patient>` | `Patient` | `ValidationError`, `DbError` |
| `find_by_id` | `(&self, id: Uuid) -> Result<Option<Patient>>` | `Option<Patient>` | `DbError` |

### Behavioral Scenarios

[HAPPY] Create patient with valid data
GIVEN valid patient data with name "Dr. Smith"
WHEN `create_patient` is called
THEN a `Patient` is returned with a generated UUID
AND the patient is retrievable via `find_by_id`

[ERROR] Create patient with empty name
GIVEN patient data with an empty `name` field
WHEN `create_patient` is called
THEN `ValidationError::MissingField("name")` is returned
AND no patient is persisted

### Invariants

- `find_by_id` never panics — always returns `Result`
- `create_patient` is idempotent on retry with the same UUID
- All returned `Patient` structs have non-empty `name` fields

### Required Test Coverage

- [ ] Happy path: create + retrieve
- [ ] Missing required fields (name, date_of_birth)
- [ ] Duplicate detection
- [ ] Concurrent creation race condition
```

## 4. Data Model Format

For each key struct, type, or database entity:

```markdown
### Patient

| Field | Type | Constraints | Default | Notes |
|-------|------|-------------|---------|-------|
| `id` | `Uuid` | PK, auto-generated | — | v4 UUID |
| `name` | `String` | NOT NULL, non-empty | — | Full legal name |
| `date_of_birth` | `NaiveDate` | NOT NULL, must be past | — | — |
| `status` | `PatientStatus` | NOT NULL | `Active` | See State Machine below |
| `created_at` | `DateTime<Utc>` | NOT NULL, immutable | `now()` | Set once on creation |

#### Validation Rules

[ERROR] Future date of birth
GIVEN a date_of_birth in the future
WHEN the patient record is validated
THEN `ValidationError::FutureDateOfBirth` is returned
```

> [!NOTE]
> Simple field constraints (NOT NULL, type, range) belong in the table.
> Complex multi-field validation or business rules should be BDD scenarios.

## 5. State Machine Format

For stateful components, combine a Mermaid diagram with a transition table:

```markdown
### Patient Status

​```mermaid
stateDiagram-v2
    [*] --> Active : create
    Active --> Inactive : deactivate
    Active --> Archived : archive
    Inactive --> Active : reactivate
    Inactive --> Archived : archive
    Archived --> [*]
​```

| From | To | Trigger | Side Effects |
|------|----|---------|--------------|
| — | Active | `create_patient()` | Audit log entry |
| Active | Inactive | `deactivate(reason)` | Notification sent, audit log |
| Active | Archived | `archive()` | Records become read-only |
| Inactive | Active | `reactivate()` | Audit log entry |
| Inactive | Archived | `archive()` | Records become read-only |
```

> [!IMPORTANT]
> Every state transition must specify its **trigger** (what causes it) and **side effects** (what else happens).
> If a transition is forbidden, document it explicitly: "Active → Active: INVALID".

## 6. API & Integration Contract Format

For external APIs, inter-service boundaries, or REST endpoints:

```markdown
### Sync API

| Endpoint | Method | Request Body | Response | Auth |
|----------|--------|-------------|----------|------|
| `/sync/push` | POST | `SyncPayload` | `SyncResult` | Bearer token |
| `/sync/pull` | GET | — | `SyncPayload` | Bearer token |

#### Behavioral Scenarios

[HAPPY] Successful sync push
GIVEN a valid SyncPayload with 5 changed records
WHEN POST `/sync/push` is called
THEN 200 OK is returned with `SyncResult.applied = 5`

[ERROR] Expired auth token
GIVEN an expired bearer token
WHEN any sync endpoint is called
THEN 401 Unauthorized is returned
AND the response includes `error: "token_expired"`

[EDGE] Conflict during push
GIVEN a SyncPayload containing a record modified on both client and server
WHEN POST `/sync/push` is called
THEN the server version wins
AND `SyncResult.conflicts` contains the conflicting record IDs
AND the client receives the server's version in the response
```

## 7. Metadata & Header

Every `spec.md` must begin with a metadata header:

```markdown
# Behavioral Specification

| Field | Value |
|-------|-------|
| **Project** | [project name] |
| **Version** | [N] |
| **Last Updated** | [date] |

> Last verified against: [short commit hash]
```

- **Version**: Increment on structural changes (new modules, removed contracts)
- **Last verified against**: Source code commit hash — managed by `/update-doc` per `doc-rules.md §5`

> [!NOTE]
> Drift detection mechanics (hash comparison, advisory warnings) are defined in
> `doc-rules.md §5`. Do not duplicate them here.

## 8. Scope Boundaries

| Question | Document | Example |
|----------|----------|---------|
| **What should it do?** | `spec.md` | "Login fails with lockout after 3 attempts" |
| **How is it structured?** | `architecture.md` | "Auth module uses argon2, depends on DB trait" |
| **What does it look like?** | `design-spec.md` | "Login screen has email + password + submit button" |
| **How is it coded?** | `coding-standard.md` | "Use `thiserror` for error enums, not `anyhow`" |

### Overlap Rules

- If a behavior is already specified in `spec.md`, the implementation plan (`ipr.md`) references it — it does not redefine it
- `architecture.md` Module Boundaries define **what** a module owns; `spec.md` defines **how** it behaves
- `design-spec.md` Interaction Flows describe UI behavior; `spec.md` describes the underlying system behavior that the UI triggers
- When in doubt: if it's testable with an assertion, it belongs in `spec.md`
