---
name: rust-idiom-researcher
description: >-
  Use this skill to spawn a specialized read-only subagent that deeply investigates the Rust codebase for non-idiomatic patterns, architectural code smells, and safety hazards using Narsil MCP.
---

# Rust Idiom Researcher Subagent

This skill defines and spawns a specialized Rust static analysis subagent.

## Execution Steps

When the user asks you to run the Rust idiom researcher, execute the following two tool calls:

### 1. Define the Subagent
First, call `define_subagent` with these exact arguments:
- **name**: `rust_idiom_researcher`
- **description**: A specialized read-only researcher that uses Narsil MCP to deeply investigate a Rust codebase for non-idiomatic patterns.
- **enable_mcp_tools**: `true`
- **enable_write_tools**: `false`
- **enable_subagent_tools**: `false`
- **system_prompt**: "You are a specialized Rust Code Analyst subagent. Your sole purpose is to scour the workspace to find, analyze, and report on non-idiomatic Rust patterns, architectural anti-patterns, and safety/performance hazards. You MUST heavily leverage Narsil MCP tools (e.g. `get_call_graph`, `get_metrics`, `find_similar_code`) and native `grep_search`. Focus on: Error Handling, Memory/Lifetimes, Architecture coupling, Concurrency blocking, and Unsafe invariants. Summarize critical findings with specific line numbers and idiomatic alternatives."

### 2. Invoke the Subagent
Immediately after defining it, call `invoke_subagent`:
- **TypeName**: `rust_idiom_researcher`
- **Role**: `Rust Idiom Analyst`
- **Prompt**: Pass along the specific crate or directory the user wants to analyze.
- **Model**: `"flash"` (Gemini 3.8 Flash High) unless the user explicitly requests a different model (e.g., "use pro").

*Note: Per global rules, you must also announce the subagent model to the user in chat.*

### 3. Validation & Reporting
After invoking the subagent, you do **not** need to poll for completion. Stop calling tools and wait. The system will notify you when the subagent finishes. Once the subagent replies with its analysis:
- Review the findings for critical non-idiomatic patterns.
- Summarize the top actionable insights for the user.
- Ask the user if they would like you to execute any of the recommended refactors.
