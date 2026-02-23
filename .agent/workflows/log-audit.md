---
description: How to audit the application logs for errors and warnings
---

# Log Audit Workflow

This workflow defines the standard process for deep analysis of application logs.
It goes beyond simple WARN/ERROR filtering to detect implicit problems across
**all severity levels** — including event ordering issues, resource leaks, and
timing anomalies visible only in DEBUG/TRACE entries.

The output is a diagnostic report that feeds directly into `/issue` for formal triage.

> [!IMPORTANT]
> This workflow is **diagnostic only** — no recommendations, no code edits, no plans.
> The only output is a structured **Log Audit Report** artifact.

## Prerequisites

- Read `architecture.md` (if present) for expected component lifecycle and event flow.
- Read `context.md` (if present) for historical decisions and known issues.
- Confirm you are operating as the **Architect** (high-reasoning model).

## Steps

### 1. Discovery

// turbo
Run the log inspection script to get the full severity breakdown:
```powershell
pwsh -File scripts/check-logs.ps1 -Level TRACE
```

This identifies available log files, line counts, and the distribution across all 5 severity levels.

> **IMPORTANT**: NEVER guess the log filename from the current date. Always use dynamic discovery.

### 1b. Lifecycle Discovery

// turbo
Run the lifecycle extraction to get COM thread and connection pool event timelines:
```powershell
pwsh -File scripts/check-logs.ps1 -Level TRACE -Lifecycle
```

This produces:
- **Thread Lifecycle**: spawn → init → started → exiting → drop sequence with anomaly detection.
- **Connection Pool**: establish/evict/reconnect events showing cache health.
- **Operation Timings**: `elapsed_ms` values for list/read/write/browse operations.

Use this output to pre-populate §3b (Event Ordering) and §3d (Resource Lifecycle) analysis.

### 1c. Deep Analysis Discovery

// turbo
Run the statistical analysis to get timing, churn, repetition, and span reports:
```powershell
pwsh -File scripts/check-logs.ps1 -Level TRACE -DeepAnalysis
```

This produces:
- **§A Timing Statistics**: min/max/avg elapsed_ms with outlier breakdown.
- **§B Connection Churn**: connection-to-refresh ratio and cache health.
- **§C Repetition**: top 10 most-repeated log messages (timestamps stripped).
- **§D Span Integrity**: grouped `opc.*` span counts.

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
- **Start with the `-Lifecycle` output from Step 1b** — it pre-extracts the COM thread timeline.
- Are lifecycle events in the correct sequence?
- Expected: `spawn → init → started → [operations] → exiting → drop → teardown`.
- Flag any out-of-order sequences (teardown before use, operations after teardown).
- Check that startup events precede operational events.

#### 3c. Timing Anomalies
- **Start with the §A Timing Statistics from Step 1c** — it pre-computes min/max/avg and outliers.
- Investigate any outliers >100ms: are they correlated with first-use warmup or genuine stalls?
- Look for unreasonable gaps between sequential operations that should be fast.
- Large delays may indicate blocking, deadlocks, resource contention, or thread starvation.

#### 3d. Resource Lifecycle
- **Start with the Connection Pool output from Step 1b** — it shows cache events.
- Track paired events: every `Connection established` should eventually have an `Evicting` or graceful shutdown.
- Flag: init without teardown (leak), double init, teardown without prior init.
- Check `spawn` count matches `init` count matches `exit` count (Sequence Check output).

#### 3e. Repetition Anomalies
- **Start with the §C Top Repeated Messages from Step 1c** — it identifies the most frequent log patterns.
- Unexpected repeated operations — retries, duplicate calls, or spin loops visible in DEBUG/TRACE.
- Identical log lines appearing in rapid succession may indicate a retry loop or polling issue.

#### 3f. Span Integrity
- **Start with the §D Span Integrity from Step 1c** — it groups all `opc.*` spans by type and count.
- Are tracing spans (`{span_name}`) properly opened and closed?
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

```markdown
# Log Audit Report

| Field | Value |
|-------|-------|
| **Date** | [Current date] |
| **Auditor** | Architect |
| **Log File** | [filename] |
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
5. **Strip ANSI** — log files contain escape codes; always strip before analysis.
