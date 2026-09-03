# Design Rules

> Loaded by `/design` workflow. Defines design modes, spec format, mockup conventions, review loop, and re-entry protocol.

## 1. Design Modes

| Mode | Tool | Mockup Format |
|------|------|---------------|
| **GUI** (web, desktop) | `generate_image` | PNG/WebP mockup images |
| **TUI** (terminal UI) | Code blocks | ASCII/box-drawing layout in `markdown` or `text` fenced blocks |
| **CLI** (command output) | Code blocks | Sample terminal output showing command → result |
| **Assets** (icons, branding) | `generate_image` | Icon/logo images at required sizes |

### Mode-Specific Guidelines

**GUI**: Generate one mockup per screen using `generate_image`. Show both default and key interaction states (hover, selected, error). Include dark/light variants if applicable.

**TUI**: Use box-drawing characters (`─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼`). Show focus states with highlight markers (`[>` or `*`). Note terminal size assumptions and color support.

**CLI**: Show full command invocation with expected output. Include help text, error output, and success output as separate examples.

**Assets**: Generate at target sizes. Common requirements:
- Favicon: 16×16, 32×32, 48×48
- App icon: 192×192, 512×512
- Logo: horizontal and square variants
- Splash screen: target device resolution

## 1.5 Wireframe Conventions *(GUI mode only)*

Wireframes are low-fidelity structural blueprints generated during Step 1.5 of the `/design` workflow. They exist to validate layout before styling is applied.

| Rule | Detail |
|------|--------|
| **Palette** | Monochrome — white background `#FFFFFF`, light gray `#E5E5E5` fills, dark gray `#333333` borders. Single `ACCENT_COLOR` for CTAs only. |
| **Text** | Greeked placeholder blocks for body copy. Short placeholder words for labels (`"Title"`, `"Button"`, `"Nav Item"`). No readable copy. |
| **Grid** | Column grid lines must be visible. Use 12-col for dashboards, 8-col for content pages, 4-col for mobile. |
| **Annotations** | All interactive elements enclosed in thin-border rectangles with short label. |
| **Naming** | Files saved as `wireframe-[screen-name].png` in `design/mockups/` |
| **Scope** | GUI mode only. Assets mode skips wireframes but requires the Brand Identity Gate (§3.6). TUI/CLI use ASCII box-drawing directly in Step 2. |

## 2. Design Spec Format

`design-spec.md` lives in a `design/` folder at the project root — a living, versioned document:

```
project-root/
├── design/
│   ├── design-brief.md          ← strategic intent (questionnaire output)
│   ├── design-spec.md           ← visual deliverables (references brief)
│   ├── mockups/
│   │   ├── wireframe-home.png   ← low-fi structural wireframes
│   │   ├── screen-home.png      ← high-fi approved mockups
│   │   └── screen-settings.png
│   └── assets/                  ← icons, logos, branding
│       ├── favicon.png
│       └── app-icon.png
```

Template:

```markdown
<!-- TEMPLATE_START: design-spec -->
## 🎨 Design Spec

| Field | Value |
|-------|-------|
| **Project** | [name] |
| **Mode** | GUI / TUI / CLI / Assets |
| **Screens** | N |
| **Version** | 1 |
| **Last Updated** | [date] |

### Design Brief
See [design-brief.md](design-brief.md) for audience, objectives, and visual direction.

### Wireframe Reference *(GUI only)*
| # | Screen Name | Wireframe | Structure Description | Status |
|---|------------|-----------|----------------------|--------|
| 1 | [name] | ![wireframe](mockups/wireframe-name.png) | [Embedded in mockup prompt] | Approved / Iterating |

### Design Tokens *(GUI only)*
| Category | Values |
|---|---|
| **Typography** | Font Family: [Primary], [Secondary]. Scales: [H1-H6 sizes] |
| **Colors** | Primary: [HEX], Surface: [HEX], Accent: [HEX], Error: [HEX] |
| **Layout** | Grid: [Cols/Gutter], Spacing Scale: [Base unit] |

### Screen Inventory
| # | Screen Name | Description | Mockup |
|---|------------|-------------|--------|
| 1 | [name] | [purpose] | ![name](mockups/screen-name.png) |

### Component Inventory
- [buttons, inputs, panels, modals, hotkeys, etc.]

### Interaction Design
- **Flows**: [User action] → [System response] → [Next screen]
- **States**: Default / Hover / Focus / Active / Disabled definitions
- **Micro-interactions**: [Loading animations, success confirmations]

### Accessibility Requirements
- **Color Contrast**: Min ratios (e.g., AA 4.5:1 for text)
- **Keyboard Navigation**: Focus order and trap prevention
- **Screen Reader**: ARIA labels for complex components

### Responsive / Resize Behavior *(GUI/TUI only)*
- [How the layout adapts to different sizes]

### Asset Inventory *(if applicable)*
| Asset | File | Sizes |
|-------|------|-------|
| Favicon | `assets/favicon.png` | 16, 32, 48 |
| App Icon | `assets/app-icon.png` | 192, 512 |

### Version History
| Version | Date | Changes |
|---------|------|---------|
| 1 | [date] | Initial design |
<!-- TEMPLATE_END -->
```

---

**Design Brief Template** — create this file as `design/design-brief.md` at the start of each `/design` session:

```markdown
<!-- TEMPLATE_START: design-brief -->
## 📋 Design Brief

| Field | Value |
|-------|-------|
| **Project** | [name] |
| **Date** | [date] |
| **Mode** | GUI / TUI / CLI / Assets |

### Audience & Objective
| Parameter | Value |
|-----------|-------|
| **Audience Profile** | [e.g., Enterprise Power User — expert-level, keyboard-heavy] |
| **Primary Objective** | [e.g., Screen 1: "Identify anomalies", Screen 2: "Export filtered report"] |
| **Interaction Complexity** | [e.g., Complex — real-time data, drag-to-reorder panels] |

### Visual Direction
| Parameter | Value |
|-----------|-------|
| **Emotional Tone** | [e.g., Clinical/Precise — dark background, monospace accents] |
| **Data Density** | [e.g., High — analytics dashboard with dense tabular data] |
| **Content Strategy** | [e.g., Real copy for headers/labels, greeked body text] |
| **Reference Anchors** | [e.g., "Grafana data density, Linear's interaction patterns, dark mode bias"] |

### Navigation & Menu Structure
| Parameter | Value |
|-----------|-------|
| **Primary Nav Pattern** | [e.g., Left sidebar (persistent, 240px)] |
| **Main Menu Items** | [e.g., Dashboard, Projects, Reports, Settings, Help] |
| **Sub-menus** | [e.g., Settings → General, Team, Billing (nested accordion)] |
| **Search Placement** | [e.g., Top bar, center-aligned, global scope] |
| **Breadcrumbs** | [e.g., Below top bar, shown on drill-down views only] |

### Brand Identity *(Assets mode: mandatory gate; GUI mode: if applicable)*
| Parameter | Value |
|-----------|-------|
| **Primary Color** | [HEX] |
| **Secondary Color** | [HEX] |
| **Accent Color** | [HEX] |
| **Typography** | [Font Family] |
| **Logo Mark** | [Description or reference] |
| **Brand Voice** | [e.g., Technical/Authoritative, Friendly/Approachable] |
<!-- TEMPLATE_END -->
```

## 3. Mockup Conventions

- **One mockup per screen** — do not combine multiple views into one image
- **Label interactive elements** — buttons, inputs, hotkeys, clickable areas
- **Show key states** — default, hover/focus, selected, error, empty
- **Use consistent naming** — `screen-[name].png` for screens, `[name].png` for assets
- **TUI box-drawing reference:**

```
┌─────────────────────────────────┐
│  Title Bar                      │
├─────────────────────────────────┤
│  Content Area                   │
│                                 │
│  [> Selected Item]              │
│     Normal Item                 │
│     Normal Item                 │
├─────────────────────────────────┤
│  Status Bar          [q]uit     │
└─────────────────────────────────┘
```

## 3.5 Wireframe Prompt Templates

### GUI Wireframe Base Prompt

Every GUI wireframe generation must use this template, filling all `[VARIABLE]` slots from the Design Brief:

```text
UX/UI wireframe, low-fidelity structural blueprint, flat monochrome design.
Background: white (#FFFFFF). Elements: light gray (#E5E5E5) fills with dark gray (#333333) borders.
Primary CTA elements: highlighted with [ACCENT_COLOR] fill.
[DATA_DENSITY] spacing (tight/balanced/spacious).
Clean [COLS]-column grid layout visible.
Viewport: [WIDTH]x[HEIGHT]px, [DEVICE] orientation.
No readable text — use greeked placeholder blocks for body copy.
Labels use short placeholder words (e.g., "Title", "Button", "Nav Item").
All interactive elements annotated with thin-border rectangles.

Screen purpose: [SCREEN_PURPOSE]
Navigation: [NAV_PATTERN] with items: [NAV_ITEMS]
Layout: [LAYOUT_DESCRIPTION]
Components: [COMPONENT_LIST]
```

### Variable Reference

| Variable | Source | Example |
|----------|--------|---------|
| `ACCENT_COLOR` | Design Brief → Primary brand color, or `#4A90D9` default | `#4A90D9` |
| `DATA_DENSITY` | Design Brief → Data Density | `tight` (High), `balanced` (Medium), `spacious` (Low) |
| `COLS` | Inferred from layout complexity | `12` (dashboard), `8` (content), `4` (mobile) |
| `WIDTH x HEIGHT` | Design Brief → Target viewport | `1440x900` (desktop), `375x812` (mobile) |
| `DEVICE` | Design Brief → Device type | `desktop`, `tablet`, `mobile` |
| `SCREEN_PURPOSE` | Design Brief → Primary Objective for this screen | `"User dashboard showing active alerts"` |
| `NAV_PATTERN` | Design Brief → Navigation & Menu Structure | `"Left sidebar (persistent)"` |
| `NAV_ITEMS` | Design Brief → Main Menu Items | `"Dashboard, Projects, Reports, Settings, Help"` |
| `LAYOUT_DESCRIPTION` | Agent-composed from requirements | `"Left sidebar (240px), top bar, 2x2 card grid main area"` |
| `COMPONENT_LIST` | Agent-composed from requirements | `"Sidebar nav (6 items), search input, 4 metric cards, data table"` |

### Assets Wireframe Base Prompt

```text
Design concept sketch, low-fidelity, monochrome with [ACCENT_COLOR] accent.
[ASSET_TYPE] for [USAGE_CONTEXT].
Style: [STYLE_CONSTRAINT].
Symbolic concept: [SYMBOLIC_INTENT].
Size: [TARGET_SIZE]px.
Background: transparent or white.
No fine detail — structural forms and shapes only.
```

### Wireframe Structure Description Template

After **"Structure Approved"**, the agent MUST generate and save a Wireframe Structure Description before proceeding to styled mockup generation. Format:

```markdown
## Wireframe Structure Description: [Screen Name]

**Viewport:** [WIDTH]x[HEIGHT]px, [DEVICE]
**Grid:** [COLS]-column, [GUTTER]px gutter
**Data Density:** [DENSITY]

### Layout Zones (top to bottom, left to right)
1. **[Zone Name]** ([dimensions]): [description of elements within]
2. **[Zone Name]** ([dimensions]): [description of elements within]

### Navigation Structure
- **Pattern**: [nav pattern]
- **Primary Items**: [menu items list]
- **Active Indicator**: [description]
- **Sub-navigation**: [description of any nested levels]
- **User Menu**: [placement and items]
- **Search**: [placement and scope]
- **Breadcrumbs**: [when shown]

### Component Details
| Component | Position | Size | States |
|-----------|----------|------|--------|
| [name] | [position] | [dimensions] | [state list] |
```

### Mockup Upgrade Prompt

Once wireframes are structure-approved, use this prompt for styled mockup generation, embedding the full Wireframe Structure Description:

```text
High-fidelity UI mockup. Follow this exact structural layout:

[PASTE WIREFRAME STRUCTURE DESCRIPTION HERE]

Design System:
- Primary Color: [HEX] | Surface: [HEX] | Accent: [HEX]
- Typography: [FONT_FAMILY], weights [WEIGHTS]
- Corner Radius: [RADIUS]px
- Spacing Base Unit: [UNIT]px

Emotional Tone: [TONE]
Brand Voice: [VOICE]

Maintain exact layout zones, proportions, and component placement
as described above. Apply the design system to transform the
structural wireframe into a polished, production-ready interface.
```

## 3.6 Brand Identity Gate *(Assets mode)*

> [!IMPORTANT]
> Before generating any assets, the brand identity **MUST** be solidified. This gate fires **once per project** — if a `design/design-brief.md` already exists with brand identity locked, subsequent asset requests inherit it automatically and the gate is skipped.

The agent must confirm or establish all fields in the `### Brand Identity` section of `design-brief.md`:

| Field | Required |
|-------|----------|
| Primary Color (HEX) | ✅ Mandatory |
| Secondary Color (HEX) | ✅ Mandatory |
| Accent Color (HEX) | ✅ Mandatory |
| Typography Family | ✅ Mandatory |
| Logo Mark | ✅ Mandatory |
| Brand Voice | ✅ Mandatory |

If any field is unset, the agent MUST gather it from the user before proceeding to `generate_image`.

## 4. Review Loop Protocol

- Each iteration: agent presents mockup → user reviews → feedback or **"Approve"**
- **"Approve"** is per-screen — can approve screen 1 while iterating on screen 2
- Track iteration count per screen (keep below 5 per screen)
- If >3 iterations on a single screen, summarize the pattern of disagreement and ask focused questions
- On **"Approve"**: save approved mockup to `design/mockups/`
- On feedback: revise specific elements, keep approved parts unchanged
- Always present **before/after** comparison when revising

## 5. Design Re-entry Protocol

Decision tree for UI problems found after design was approved:

```
UI doesn't look right
├─ Implementation doesn't match approved mockup?
│  └─ /issue (Type: bug) → /plan-making (fix code)
├─ User wants to change the approved design?
│  └─ /design (Revision mode) → update Design Spec → /plan-making
├─ Planning reveals design won't work technically?
│  └─ STOP plan → /design (Revision mode) → resume /plan-making
```

### Revision Mode Rules

1. Re-enter `/design` with the existing Design Spec
2. Scope: specify which screens/assets are being revised (not full redesign)
3. Mark revised screens with `[REVISED]` tag in the Screen Inventory
4. Keep approved screens unchanged
5. Same review loop applies (§4)
6. Bump `Version` in Design Spec header
7. Add entry to Version History table
