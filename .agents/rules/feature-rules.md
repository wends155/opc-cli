# Feature Rules

> Loaded by `/feature` workflow. Defines classification criteria, report format, and architectural fit assessment.

## 1. Classification

### Category

| Category | Definition | Example |
|----------|-----------|---------|
| `enhancement` | Improve existing functionality | Better error messages, performance optimization |
| `new-capability` | Add entirely new functionality | CSV export, dark mode, new API endpoint |
| `integration` | Connect to external system or service | Database migration, third-party API, webhook |
| `refactor` | Restructure without changing behavior | Extract module, rename crate, simplify architecture |

### Priority

| Priority | Criteria | Implication |
|----------|----------|-------------|
| `must-have` | Blocks release or critical user workflow | Plan immediately after research |
| `should-have` | Important but not blocking, clear user value | Plan in current cycle |
| `nice-to-have` | Bonus, low urgency, incremental improvement | Backlog — plan when capacity allows |

## 2. Feature Research Report Format

<!-- TEMPLATE_START: feature-research-report -->
```markdown
## ✨ Feature Research Report

| Field          | Value                                       |
|----------------|---------------------------------------------|
| **Feature**    | [name]                                      |
| **Category**   | enhancement / new-capability / integration  |
| **Component**  | [affected area]                             |
| **Priority**   | must-have / should-have / nice-to-have      |
| **Complexity** | small / medium / large                      |
| **Filed**      | [date]                                      |

### Description
[Clear restatement of the desired feature in the user's own words]

### Current State
- **Existing code:** [relevant modules/files, if any]
- **Related history:** [prior decisions from context.md, if any]
- **Gaps:** [what's missing to support this feature]

### Ecosystem Research
- **Libraries evaluated:** [list with brief notes]
- **Recommended dependency:** [name + reasoning], or "None — custom implementation preferred"

### Approaches

#### Option A: [Name]
- **Description:** [how it works]
- **Pros:** [advantages]
- **Cons:** [disadvantages]
- **Complexity:** [small / medium / large]

#### Option B: [Name]
- **Description:** [how it works]
- **Pros:** [advantages]
- **Cons:** [disadvantages]
- **Complexity:** [small / medium / large]

### Recommendation
[Which option and why. Include any caveats or conditions.]

### Architectural Fit
[Results of §3 assessment — see below]

### Risks & Constraints
- [List risks, trade-offs, and hard constraints]

### Open Questions
- [Ambiguities or decisions that need user input]
```
<!-- TEMPLATE_END -->

## 3. Architectural Fit Assessment

When `architecture.md` exists, evaluate the feature against these checks:

| Check | Question |
|-------|----------|
| **Module Ownership** | Does the feature fit within an existing module's "Owns" boundary, or does it need a new module? |
| **Dependency Direction** | Does it respect the May Import / Must NOT Import rules? |
| **Interface Impact** | Does it require new cross-module interfaces or traits? |
| **External Dependencies** | Does it introduce a new dependency? Where does it sit in the dependency graph? |
| **Mockability** | Would it violate any existing module boundary or mockability contract? |
| **Layer Placement** | Which architectural layer does this feature belong in (handler, service, repository)? |

If `architecture.md` does not exist, note this as a gap and recommend running `/architecture` first for non-trivial features.
