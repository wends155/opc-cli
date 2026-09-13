---
description: Deep analysis of application logs for errors, timing anomalies, and resource leaks (Diagnostic). Triggers: /log-audit, check logs
---

# Log Audit Workflow (Generic)

This workflow defines the standard process for deep analysis of application logs.
It goes beyond simple WARN/ERROR filtering to detect implicit problems across
**all severity levels** — including event ordering issues, resource leaks, and
timing anomalies visible only in DEBUG/TRACE entries.

The output is a diagnostic report that feeds directly into `/issue` for formal triage.

> [!IMPORTANT]
> This is a **generic** version. Projects with specialized log formats (e.g., OPC,
> gRPC, database) may have a project-specific `log-audit.md` that overrides this.
> Check the project's `.agents/workflows/` first.

> [!IMPORTANT]
> This workflow is **diagnostic only** — no recommendations, no code edits, no plans.
> The only output is a structured **Log Audit Report** artifact.

## Prerequisites

> [!IMPORTANT]
> **Execution Discipline:** You **MUST** use the `view_file` tool to read all listed rule files (e.g., `.agents/rules/...`) before starting Step 1. Do not rely on internal memory.

- Read `architecture.md` (if present) for expected component lifecycle and event flow.
- Read `context.md` (if present) for historical decisions and known issues.
- Confirm you are operating as the **Architect** (high-reasoning model).

## Steps

### 1. Discovery

// turbo
Run these log inspection commands (auto-runnable):

```
make audit-log-levels
```
```
make audit-log-errors
```

This produces a severity count per log file and the most recent warning/error entries.

> **IMPORTANT**: If the project has logs in a non-default directory, use `-LogDir <path>`.

### 1b. Lifecycle Discovery

// turbo
Run the lifecycle extraction commands:

```
make audit-log-lifecycle
```
```
make audit-log-network
```
```
make audit-log-latency
```

This produces:
- **Thread Lifecycle**: spawn → init → started → exiting → drop sequence.
- **Connection Pool**: establish/evict/reconnect events showing cache health.
- **Operation Timings**: elapsed/duration/latency values.

Use this output to pre-populate §3b (Event Ordering) and §3d (Resource Lifecycle) analysis.

### 1c. Deep Analysis Discovery

// turbo
Run these deep analysis commands:

```
make metrics-log-count
```
```
make metrics-log-raw
```

Agent performs statistical analysis on the output:
- **Timing:** extract numeric values from timing entries, compute min/max/avg, flag outliers >100ms
- **Connection churn:** count establish vs close vs reconnect events; flag high reconnect ratios
- **Repetition:** read full log with `view_file`, strip timestamps, group identical messages by count
- **Span integrity:** group `\w+\.\w+\{` patterns by span type and verify close/open balance

Use this output to pre-populate §3c (Timing), §3e (Repetition), and §3f (Span Integrity) analysis.

### 2. Full Content Ingestion

Read the **entire** log file (all levels), stripping ANSI escape codes:
```powershell
$latest = Get-ChildItem logs -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Get-Content $latest.FullName
```

Build a mental timeline of events from first to last entry. Note the overall session structure:
startup → operations → shutdown.

### 3. Deep Analysis

Analyze the log across **6 dimensions**. Problems can exist at ANY severity level — not just WARN/ERROR.

#### 3a. Explicit Failures
- Scan for `WARN` and `ERROR` level entries.
- These are direct failure signals — record each one.

#### 3b. Event Ordering
- **Start with the `-Lifecycle` output from Step 1b** — it pre-extracts thread timelines.
- Are lifecycle events in the correct sequence?
- Expected: `init → started → [operations] → shutting down → cleanup`.
- Flag any out-of-order sequences (cleanup before use, operations after shutdown).
- Check that startup events precede operational events.

#### 3c. Timing Anomalies
- **Start with the §A Timing Statistics from Step 1c** — it pre-computes min/max/avg and outliers.
- Investigate any outliers >100ms: are they correlated with first-use warmup or genuine stalls?
- Look for unreasonable gaps between sequential operations that should be fast.
- Large delays may indicate blocking, deadlocks, resource contention, or thread starvation.

#### 3d. Resource Lifecycle
- **Start with the Connection Pool output from Step 1b** — it shows cache events.
- Track paired events: every `connection established` should eventually have a close or shutdown.
- Flag: init without teardown (leak), double init, teardown without prior init.
- Check event counts balance (spawns match inits match exits).

#### 3e. Repetition Anomalies
- **Start with the §C Top Repeated Messages from Step 1c** — it identifies the most frequent log patterns.
- Unexpected repeated operations — retries, duplicate calls, or spin loops visible in DEBUG/TRACE.
- Identical log lines appearing in rapid succession may indicate a retry loop or polling issue.

#### 3f. Span Integrity
- **Start with the §D Span Integrity from Step 1c** — it groups tracing spans by type and count.
- Are tracing spans properly opened and closed?
- Orphaned or mismatched spans indicate control flow issues.
- Check that nested span entries are logically consistent.

### 4. Problem Synthesis

For each detected problem, record:

| Field | Description |
|-------|-------------|
| **What** | Concise description of the anomaly |
| **Where** | Specific log line(s) and timestamp(s) |
| **Dimension** | Which analysis dimension (3a–3f) |
| **Severity** | `critical` / `high` / `medium` / `low` |
| **Hypothesis** | Initial root cause guess — **NOT a recommendation** |

### 5. Generate Report

Produce a `log_audit_report.md` artifact:

> [!NOTE]
> Once the artifact is written, you **MUST** provide a clickable markdown link to it in your final chat response (e.g., `[Log Audit Report](file:///absolute/path/to/log_audit_report.md)`).

```markdown
# Log Audit Report

| Field | Value |
|-------|-------|
| **Date** | [Current date] |
| **Auditor** | Architect |
| **Log File** | [filename] |
| **Format** | [detected format] |
| **Line Count** | [total lines] |

## Severity Breakdown
| TRACE | DEBUG | INFO | WARN | ERROR |
|-------|-------|------|------|-------|
| N     | N     | N    | N    | N     |

## ⚠️ Problems Detected

### Problem 1: [Short Title]
| Field | Value |
|-------|-------|
| **Severity** | critical / high / medium / low |
| **Dimension** | Explicit Failures / Event Ordering / Timing / Resource / Repetition / Span |
| **Log Lines** | [timestamps and content] |

**Description:** [What was observed]

**Hypothesis:** [Initial root cause guess — NO recommendations]

---

## No Issues Found
[If clean: "No anomalies detected across all 6 analysis dimensions."]
```

> [!CAUTION]
> Do NOT include recommendations or proposed fixes. This report is strictly
> diagnostic. Recommendations are the responsibility of `/issue` → `/plan-making`.

### 6. Handoff

Present the report to the user.

- If problems were found:
  > 🔍 **Log Audit Complete.** [N] problem(s) detected.
  > Reply with **`/issue`** to formally triage the findings.

- If clean:
  > ✅ **Log Audit Complete.** No anomalies detected.

## Rules

1. **No code edits** — this is a diagnostic-only workflow.
2. **No recommendations** — only problems and hypotheses. Fixes go through `/issue`.
3. **All levels matter** — do not skip DEBUG/TRACE entries. Problems hide there.
4. **Always pause** — the user must explicitly invoke `/issue` to proceed.
5. **Strip ANSI** — log files may contain escape codes; always strip before analysis.
6. **Check for project override** — if the project has its own `log-audit.md`, use that instead.


