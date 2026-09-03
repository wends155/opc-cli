---
description: Fine-grained algorithm design and analysis (Pre-Think Phase)
---

# /ago — Algorithm Design Workflow

Deep, structured analysis of domain-specific algorithms before committing to
an architecture. Produces `algorithm.md` with formal problem definitions,
complexity analysis, tradeoff matrices, and integration contracts.

> [!IMPORTANT]
> This workflow produces an artifact (`algorithm.md`). The `/architecture`
> workflow reads it to place algorithms within the system's module boundaries
> and verify integration contracts — preventing orphaned algorithms.

## Trigger

User invokes: `/ago <topic>`

Example: `/ago truck routing with time windows and capacity constraints`

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed rule files (e.g., `.agents/rules/...`) before starting Step 1. Do not rely on internal memory.

- Read `.agents/rules/algo-rules.md` for required sections and integration contract format.
- Read `context.md` (if present) for existing project context.
- If `/brainstorm` was run, carry forward the conclusions about scope and constraints.
- Confirm you are operating as the **Architect** (high-reasoning model).

## Steps

### 1. Problem Framing

Decompose the algorithmic challenge:

- **Classification**: What type of problem? (optimization, CSP, graph, scheduling, ML — see `algo-rules.md` §5)
- **Formal definition**: Inputs, outputs, constraints, objective function
- **Scale**: What is the target N? (100? 10,000? 1M?)
- **Quality requirements**: Exact solution needed? Or is approximate/heuristic acceptable?
- **Multiple algorithms?** If the system needs several algorithms, identify each one and their relationships

Use **Sequential Thinking MCP** to reason through problem decomposition before responding.

> [!TIP]
> Ask the user clarifying questions. The problem definition shapes everything downstream.
> "Is this a single-depot or multi-depot problem?" changes the entire algorithm space.

### 2. Landscape Research

Assess the existing solution landscape:

- **Context7 MCP**: Check library documentation for existing implementations
  (e.g., OR-Tools, petgraph, rayon, ndarray)
- **Web Search**: Find academic papers, known algorithms, benchmark results
- **Existing crates/packages**: Is there a battle-tested implementation? What's its API?

Key questions to answer:
- Is this a solved problem with known optimal algorithms?
- Is it NP-hard? If so, what approximation ratios are achievable?
- Are there off-the-shelf libraries, or must we build from scratch?

### 3. Deep Analysis

For each candidate algorithm, analyze:

| Dimension | Analysis |
|-----------|----------|
| **Time complexity** | Big-O theoretical + practical at target N |
| **Space complexity** | Memory footprint at target N |
| **Correctness** | Exact, approximate (with ratio), or heuristic |
| **Scalability** | At what N does it break? Can it be parallelized? |
| **Implementation effort** | Lines of code estimate, dependency weight |
| **Maintainability** | How hard to debug, extend, or swap later? |

Use **Sequential Thinking MCP** for multi-factor tradeoff analysis:
- Weigh the dimensions against project constraints
- Consider: what if requirements change? Which algo adapts best?
- Think adversarially: what inputs would break each candidate?

### 4. Selection & Justification

Build the tradeoff matrix comparing all candidates (per `algo-rules.md` §1.4):

```markdown
| Criterion | Algo A | Algo B | Algo C |
|-----------|--------|--------|--------|
| Time (N=1000) | O(n²) ~100ms | O(n log n) ~10ms | O(n³) ~5s |
| Optimality | Exact | ~95% approx | Exact |
| Implementation | Medium | Easy | Hard |
| Maintainability | High | High | Low |
```

Then:
- Select the approach with clear rationale
- Write pseudocode (language-agnostic, per `algo-rules.md` §1.6)
- Document edge cases (per `algo-rules.md` §1.7)

### 5. Integration Contract

For **each** algorithm, define the integration contract (per `algo-rules.md` §3):

- **Module Home**: Where this algorithm will live in the codebase
- **Input**: Data type + which module provides it
- **Output**: Data type + which module(s) consume it
- **Error Surface**: What can fail and the error types
- **Performance Budget**: Max latency/memory the system can tolerate
- **Fallback**: What happens on failure or timeout

For **multiple algorithms**, additionally:
- **Interaction Graph**: Mermaid diagram of data flow between algorithms
- **Pipeline Ordering**: Which runs first, dependencies, parallelism opportunities
- **Shared Data Structures**: Common types used across algorithms

> [!CAUTION]
> The integration contract is what prevents orphaning. Every module referenced
> (input source, output consumer) must exist in the eventual architecture.
> The `/architecture` workflow will verify this.

### 6. Write algorithm.md

Generate `algorithm.md` in the project root with all sections per `algo-rules.md`.

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your final chat response (e.g., `[algorithm.md](file:///absolute/path/to/algorithm.md)`).

Structure for single algorithm:
```markdown
# Algorithm: [Name]
## Problem Statement
## Algorithm Candidates
## Complexity Analysis
## Tradeoff Matrix
## Selected Approach
## Pseudocode
## Edge Cases
## Integration Contract
```

Structure for multiple algorithms:
```markdown
# Algorithms
## Algorithm Interaction Graph
## Pipeline Ordering
## Shared Data Structures

## Algorithm 1: [Name]
### Problem Statement ... ### Integration Contract

## Algorithm 2: [Name]
### Problem Statement ... ### Integration Contract
```

### 7. Pause for Review

End with:

> 🛑 **Algorithm Design Complete.**
> Please review `algorithm.md`. You can:
> - **Adjust** any analysis or selection
> - **Add** domain knowledge the LLM may have missed
> - **Challenge** the tradeoff weightings
>
> When satisfied, reply with:
> - **"/architecture"** to formalize the system structure (algorithm.md will be consumed)
> - **"/design"** to explore UI/UX before architecture
> - **"Plan"** to proceed to implementation planning directly

**Do NOT proceed to implementation without user approval.**

## Rules

1. **Deep analysis required** — do not skip complexity analysis or tradeoff matrices.
2. **No implementation code** — pseudocode only. Implementation goes through `/plan-making`.
3. **Integration contracts are mandatory** — every algorithm must declare its system placement.
4. **Multiple algorithms supported** — identify ALL algorithms needed, not just the primary one.
5. **Use MCP tools** — Sequential Thinking for analysis, Context7 for library research, web search for papers.
6. **Challenge assumptions** — probe the user's constraints. "Do you really need exact solutions?"
7. **Carry context forward** — when transitioning to `/architecture`, reference algorithm.md conclusions.


