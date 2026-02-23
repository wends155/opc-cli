# `opc-cli` Logfile Format Reference

This document serves as the canonical source of truth for the application's file logging format. Log analysis tools and agent workflows (like `/log-audit` and `check-logs.ps1`) must use these rules to parse log files accurately.

## 1. Line Format Structure

Each log event is written as a single, newline-terminated string formatted as follows:

```
{ISO8601_UTC_TIMESTAMP} {LEVEL:>5} {span_chain}: {target_module}: {message}
```

### Components
- **Timestamp:** Fixed ISO-8601 UTC format terminating with `Z` (e.g., `2026-02-22T03:13:24.527476Z`).
- **Level:** Right-aligned to 5 characters (`TRACE`, `DEBUG`, ` INFO`, ` WARN`, `ERROR`).
- **Span Chain:** Semicolon-delimited stack of active tracing spans. Span names are followed by comma-separated `{field=value}` pairs (e.g., `opc.list_servers{host=localhost}`). Empty if no spans are active.
- **Target:** The Rust module path where the event originated (e.g., `opc_da_client::com_guard`).
- **Message:** The text payload of the event.

## 2. Encoding and Cleansing

- **Encoding:** UTF-8
- **ANSI Codes:** **STRICTLY PROHIBITED**. The file writer layer enforces `.with_ansi(false)`. Log lines consist purely of printable text, eliminating the need for complex regex stripping during analysis.
- **Interleaving:** A dedicated non-blocking writer thread ensures all writes are atomic at the event level. One event = strictly one line in the log file.

## 3. Correctness Criteria

For automated validation, every valid log line **MUST** match the following regular expression:

```regex
^\d{4}-\d{2}-\d{2}T[\d:.]+Z\s+(TRACE|DEBUG|INFO|WARN|ERROR)\s+.+$
```

Any line failing this regex indicates either a multiline message payload (which should be avoided in structured logging) or a line interleaving/corruption bug in the appender.

## 4. Example Lines

**Example 1: Top-level event (no spans)**
```
2026-02-22T03:13:24.527476Z  INFO opc_cli: Starting OPC CLI
```

**Example 2: Deeply nested event with fields**
```
2026-02-22T03:13:25.279812Z DEBUG opc.list_servers{host=localhost}: opc_da_client::com_guard: COM MTA initialized
```
