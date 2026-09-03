---
description: Iterative UI/UX design with mockup review loop (Pre-Think Phase)
---

# Design Workflow

This workflow defines the standard process for designing user interfaces
and visual assets **before** any implementation begins. It is the only
workflow with an **iterative review loop** — the user and agent go back
and forth on mockups until each screen is approved.

> [!IMPORTANT]
> This workflow is **design-only** — no code edits, no implementation.
> The output is a **Design Spec** (`design/design-spec.md`) that feeds
> into `/plan-making`.

> [!NOTE]
> The Design Spec produced here is the **input artifact** for `/plan-making`.
> The implementation plan references specific mockups for each component.

## Trigger

User invokes: `/design <description>`

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed rule files (e.g., `.agents/rules/...`) before starting Step 1. Do not rely on internal memory.

- Read `.agents/rules/design-rules.md` for design modes, spec format, mockup conventions, and review protocol.
- Read `architecture.md` (if present) for project structure and module boundaries.
- Read `design/design-spec.md` (if present) for existing designs (revision mode).
- Confirm you are operating as the **Architect** role.

## Steps

### 1. Gather Design Intent

Determine the scope:

- **New design** or **revision** of an existing Design Spec?
- If revision: load existing `design/design-spec.md`, identify which screens/assets to revise.

Determine the design mode per `design-rules.md` §1:

| Mode | When |
|------|------|
| **GUI** | Web interfaces, desktop apps |
| **TUI** | Terminal UIs (Ratatui, crossterm, etc.) |
| **CLI** | Command-line output format |
| **Assets** | Favicon, app icon, logo, splash screen |

> [!TIP]
> **Agency Skill JIT Loading (Phase A: Intent & Discovery)**
> If mode is **GUI** or **Assets**, use `view_file` to load Phase A skills:
> - `.gemini/skills/agency-ux-researcher/SKILL.md`
> - `.gemini/skills/agency-brand-guardian/SKILL.md`
> - `.gemini/skills/agency-visual-storyteller/SKILL.md`

Gather requirements using the **Design Intent Questionnaire**:

#### GUI Mode — 8 Dimensions

| # | Dimension | Question |
|---|-----------|----------|
| 1 | **Audience Persona** | Who is the primary user? Expert (shortcuts, dense data), Casual (guidance, spacious layout), or Accessibility-dependent (WCAG AA+)? |
| 2 | **Primary Objective** | What is the single most important action on each screen? (e.g., "Sign up", "Export report") |
| 3 | **Data Density** | High (analytics dashboards, admin panels), Medium (content apps), or Low (marketing pages, onboarding)? |
| 4 | **Emotional Tone** | Corporate/Trustworthy, Playful/Whimsical, Clinical/Precise, Aggressive/Kinetic, Warm/Organic, or Minimal/Zen? |
| 5 | **Content Strategy** | Real copy or placeholder content? What's the copy tone? (Technical, Friendly, Formal) |
| 6 | **Reference Anchors** | Products to emulate or avoid? (e.g., "like Stripe but darker") |
| 7 | **Interaction Complexity** | Simple (mostly static), Medium (forms, modals), or Complex (drag-and-drop, real-time, multi-panel)? |
| 8 | **Navigation & Menu Structure** | Primary nav pattern? (Top bar, Left sidebar, Bottom tabs, Hamburger). Main menu items? Sub-menus, nested levels, breadcrumbs? Search placement? |

#### Assets Mode — 5 Dimensions (Brand Identity Gate)

> [!IMPORTANT]
> **Brand Identity Gate:** Brand identity MUST be solidified before any asset generation. See `design-rules.md` §3.6. Wireframes are skipped for Assets mode — the Gate is the prerequisite instead.

| # | Dimension | Question |
|---|-----------|----------|
| 1 | **Brand Identity** *(mandatory gate)* | Define or confirm: Primary/secondary/accent colors (HEX), typography family, logo mark, brand voice. |
| 2 | **Brand Context** | Extending existing brand or creating from scratch? |
| 3 | **Usage Context** | Where will the asset appear? (favicon tab, app store, print, social media) |
| 4 | **Symbolic Intent** | What concept should the asset communicate? (Speed, Security, Growth, Community) |
| 5 | **Style Constraint** | Flat/geometric, illustrative, photorealistic, or abstract? |

#### TUI/CLI Modes
- For TUI: terminal size assumptions, color support (256/truecolor), mouse support?
- For CLI: output format, verbosity levels, color support?

Persist questionnaire answers to `design/design-brief.md` using the template from `design-rules.md` §2 before proceeding.

If the description is too vague, **ask clarifying questions immediately**
before proceeding to Step 2.

For **medium/large** designs (multiple screens), the Architect **SHOULD** use
`sequentialthinking` to structure the screen inventory and interaction flows
before generating mockups.

### 1.5 Generate Wireframes *(GUI mode only)*

> [!TIP]
> **Agency Skill JIT Loading (Wireframe Phase)**
> Load before wireframe generation:
> - `.gemini/skills/agency-image-prompt-engineer/SKILL.md` (Wireframe Prompt Scaffold section)

Using the Design Brief answers and the prompt templates from `design-rules.md` §3.5:

1. Compose wireframe prompts by filling the GUI Wireframe Base Prompt template variables.
2. Generate low-fidelity wireframe images using `generate_image` (one per screen).
3. Present wireframes to the user for **structural approval only** — layout, not style.
4. User reviews:
   - **Feedback**: Revise layout (add sidebar, move CTA, split form, change nav pattern).
   - **"Structure Approved"**: Proceed to styled mockup generation.
5. On **"Structure Approved"**: Generate a **Wireframe Structure Description** per `design-rules.md` §3.5 and save it alongside the wireframe image for use as the mockup layout constraint.
6. Add wireframe entry to the `Wireframe Reference` table in `design/design-spec.md`.

> [!CAUTION]
> The agent **MUST NOT** proceed to Step 2 (styled mockups) until **"Structure Approved"** is given for each screen.

> [!NOTE]
> **Skip conditions:** This step is skipped for:
> - **Assets mode** (wireframes not applicable — Brand Identity Gate applies instead)
> - **TUI/CLI modes** (use ASCII/box-drawing directly in Step 2)
> - **Small designs** (≤2 screens) when the user explicitly says "skip wireframes"

### 2. Generate Initial Mockups

> [!TIP]
> **Agency Skill JIT Loading (Phase B: Foundation & Structure)**
> - For **GUI** mode, load Phase B GUI skills:
>   - `.gemini/skills/agency-ux-architect/SKILL.md`
>   - `.gemini/skills/agency-ui-designer/SKILL.md`
> - For **Assets** mode, load Phase B Asset skills:
>   - `.gemini/skills/agency-inclusive-visuals-specialist/SKILL.md`
>   - `.gemini/skills/agency-image-prompt-engineer/SKILL.md`

Based on the design mode:

- **GUI**: use `generate_image` to create mockup images (one per screen). For screens with an approved wireframe, use the **Mockup Upgrade Prompt** from `design-rules.md` §3.5, embedding the Wireframe Structure Description verbatim to preserve the approved layout.
- **TUI**: use ASCII/box-drawing in fenced code blocks per `design-rules.md` §3
- **CLI**: show sample command invocation + expected output
- **Assets**: use `generate_image` for icons/logos at target sizes

Label all interactive elements (buttons, inputs, hotkeys, clickable areas).
Show both default state and key interaction states where applicable.

#### MCP-Enhanced Design *(when available)*

If **Narsil MCP** is available, use it to understand existing UI code:

| Tool | Purpose |
|------|---------|
| `search_code` | Find existing UI components or templates |
| `find_symbols` | Discover existing component API surface |
| `get_project_structure` | Understand where UI code lives |

### 3. Review Loop

> [!TIP]
> **Agency Skill JIT Loading (Phase C: Polish & Delight)**
> If mode is **GUI**, load Phase C skills to enhance personality and interaction states before generating final revisions:
> - `.gemini/skills/agency-whimsy-injector/SKILL.md`

Present mockups to the user and iterate per `design-rules.md` §4:

1. Present mockup(s) for review
2. User reviews → gives feedback or says **"Approve"**
3. If feedback: revise specific elements, keep approved parts unchanged
4. Always present **before/after** comparison when revising
5. Repeat until all screens/assets are approved

> [!TIP]
> "Approve" is per-screen, not per-session. You can approve screen 1
> while still iterating on screen 2.

> [!CAUTION]
> If iteration count exceeds 3 on a single screen, pause and summarize
> the pattern of disagreement. Ask focused questions to converge.

### 4. Produce Design Spec

Once all screens/assets are approved:

1. Create `design/` folder with `mockups/` and `assets/` subdirectories
2. Save approved mockup images to `design/mockups/`
3. Save approved asset images to `design/assets/`
4. Fill `design/design-spec.md` per `design-rules.md` §2

Include:
- Screen inventory with relative path references to mockups
- Component inventory (buttons, inputs, panels, modals, hotkeys)
- Interaction flows (user action → system response → next screen)
- Responsive/resize behavior (GUI/TUI only)
- Asset inventory (if applicable)
- Version history

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your final chat response (e.g., `[Design Spec](file:///absolute/path/to/design-spec.md)`).

For **revision mode**: bump the version, mark revised screens with `[REVISED]`,
add entry to version history per `design-rules.md` §5.

Present the complete Design Spec to the user for final review.

### 5. Handoff

End with:

> 🛑 **Design Complete.**
> Please review the Design Spec above. You can:
> - **Revise** specific screens or assets
> - **Add** screens or interactions not yet captured
>
> When satisfied, reply with **"Plan"** to proceed to `/plan-making`.

**Do NOT proceed to planning until the user explicitly approves the Design Spec.**

## Rules

1. **No code edits** — this is a design-only workflow.
2. **No planning** — do not produce implementation steps or blueprints.
3. **Always pause** — the user must explicitly say "Plan" to move forward.
4. **Use `generate_image`** for GUI/Assets, code blocks for TUI/CLI.
5. **Always present before/after** when revising mockups.
6. **Per-screen approval** — "Approve" applies to individual screens, not the entire spec.
7. **Re-entry is scoped** — revision mode targets specific screens, not full redesign (per §5).
8. `/design` can be re-entered from `/plan-making`, `/audit`, or `/issue` per `design-rules.md` §5.
9. **"Structure Approved" gate** — for GUI mode, the agent MUST NOT generate styled mockups until the user says "Structure Approved" for each screen's wireframe (Step 1.5).
10. **Design Brief persistence** — questionnaire answers MUST be saved to `design/design-brief.md` using the template in `design-rules.md` §2 before any generation begins.


