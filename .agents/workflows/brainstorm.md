---
description: Conversational ideation and feasibility exploration (Pre-Think Phase)
---

# /brainstorm — Ideation Workflow

Explore ideas, assess feasibility, and shape project concepts through guided
conversation before committing to formal documentation.

> [!NOTE]
> This is a **conversational workflow** — no artifacts are produced.
> No Planning Gate applies. The user is exploring, not committing.

## Trigger

User invokes: `/brainstorm <topic>`

Example: `/brainstorm an OPC UA client library in Rust`

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed rule files (e.g., `.agents/rules/...`) before starting Step 1. Do not rely on internal memory.

- Read `context.md` (if present) for existing project history.
- Confirm you are operating as the **Architect** (high-reasoning model).

## How It Works

This is not a step-by-step process. It's a guided conversation that explores
dimensions of the idea naturally. The Architect uses **Sequential Thinking MCP**
internally to reason about feasibility, tradeoffs, and alternatives before each
response — but the user sees only the conversational output.

### Exploration Dimensions

Cover these over the course of the conversation. Order is flexible — follow the
natural flow of discussion, not a rigid checklist.

| Dimension | What to explore |
|-----------|----------------|
| **Feasibility** | Is this realistic? What exists already? Competing solutions? Open-source landscape? |
| **Requirements** | What problem does this solve? Who's the user? What's the scope? |
| **Features** | Core vs nice-to-have. MVP definition. Feature priority and phasing |
| **Stack** | Language, framework, key dependencies. Why this over alternatives? |
| **Architecture** | Monolith vs modular? Library vs service? Plugin system? Layering? |
| **Deployment** | CLI tool? Library crate? Docker? Cloud? Embedded? Distribution? |
| **Constraints** | Performance targets, security, compliance, licensing, platform support |
| **Risks** | Technical unknowns, skill gaps, maintenance burden, ecosystem maturity |

### Conversation Guidelines

1. **Start broad, narrow gradually** — begin with feasibility and requirements,
   then dive into stack and architecture as the idea solidifies.

2. **Challenge assumptions** — if the user proposes a stack, ask why.
   If a feature seems risky, flag it. Be a thinking partner, not a yes-machine.

3. **Use MCP tools for validation** — don't guess about ecosystems:
   - **Context7**: Check library docs, API surfaces, version status
   - **Sequential Thinking**: Reason through multi-factor tradeoffs internally
   - **Narsil**: If referencing an existing codebase, use code analysis

4. **Propose alternatives** — "Have you considered X?" is always welcome.
   Present tradeoffs as tables when comparing options.

5. **Summarize periodically** — after 3–4 exchanges, recap where the idea stands:
   > "So far we've landed on: Rust async client, `tokio` runtime, no-std optional,
   > targeting embedded Linux. Still open: transport layer choice, security model."

6. **Don't over-formalize** — this is brainstorming. Bullet points and quick
   tables are fine. No need for headers, section numbers, or formal structure.

## Exit Conditions

When the idea feels solid enough, the user chooses how to proceed:

| User says | What happens |
|-----------|-------------|
| `/architecture` | Transition to formal architecture documentation. The Architect carries all brainstorm context forward into the `/architecture` workflow. |
| `/feature <name>` | Route a specific feature idea through the `/feature` investigation workflow. |
| `save` | Compress the key decisions and conclusions to `context.md` for future reference. |
| *(conversation ends)* | Nothing persisted. The brainstorm was exploratory only. |

> [!IMPORTANT]
> When transitioning to `/architecture`, explicitly reference the brainstorm
> conclusions in the architecture document's Project Overview and Project
> Objectives sections. Don't make the user repeat themselves.

## Rules

1. **No artifacts** — do not create files, plans, or reports during brainstorming.
2. **No code** — do not write implementation code. Pseudocode for illustration is fine.
3. **Stay conversational** — avoid walls of text. Keep responses focused and interactive.
4. **Use Sequential Thinking internally** — reason before responding, but show only conclusions.
5. **Validate claims** — use Context7 MCP to verify library status, API surfaces, and ecosystem facts.
6. **Respect the exit** — when the user transitions to `/architecture`, carry context forward seamlessly.


