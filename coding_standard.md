# Rust Coding Standards & Latest Practices

> **Applies to:** All Rust codebases within this workspace.
> **Last updated:** 2026-02-20
> **Rust Edition:** 2024 · **Minimum Rust Version:** 1.93.1

---

## 1. Context

As a data engineering consultancy specializing in Rust, we establish clear coding standards
that ensure high-quality, maintainable, and performant code following the latest Rust idioms.

## 2. Decision

We adopt comprehensive Rust coding standards that prioritize the latest Rust features,
best practices, and performance optimizations while maintaining code quality and consistency
across all projects.

---

## 3. Version & Toolchain

| Item | Standard |
| :--- | :--- |
| **Rust Version** | Latest stable (currently **1.93.1**) |
| **Edition** | `2024` for all new projects |
| **Toolchain Manager** | `rustup` |
| **Required Components** | `rustfmt`, `clippy`, `rust-analyzer` |

> [!IMPORTANT]
> Pin `rust-version` in `Cargo.toml` to prevent accidental MSRV regressions.

---

## 4. Code Quality Gate (Zero-Exit Requirement)

Every PR / commit **must** pass all three gates before merge:

```sh
cargo fmt  --all -- --check   # Gate 1: Formatting
cargo clippy --all-targets --all-features -- -D warnings  # Gate 2: Linting
cargo test  --all-features    # Gate 3: Tests
```

| Metric | Target |
| :--- | :--- |
| Formatting | 100% `rustfmt` compliance |
| Linting | Zero `clippy` warnings (deny mode) |
| Documentation | 100% of public APIs documented |
| Test coverage | 100% of public functions tested |
| Benchmarks | Critical paths benchmarked with Criterion |

---

## 5. Project Configuration

### 5.1 `rustfmt.toml`

```toml
edition = "2024"
max_width = 100
tab_spaces = 4
newline_style = "Unix"
use_small_heuristics = "Default"
imports_granularity = "Crate"
group_imports = "StdExternalCrate"
```

### 5.2 Clippy Configuration (Strictest)

Configure via attributes in `lib.rs` / `main.rs` (or a workspace `Cargo.toml` `[lints]` table):

```rust
// lib.rs / main.rs — strictest lint configuration
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(clippy::missing_docs_in_private_items)]
#![deny(clippy::missing_errors_doc)]
#![deny(clippy::missing_panics_doc)]
```

Or, **preferably**, use the workspace-level `[lints]` table (Rust 1.74+):

```toml
# Cargo.toml (workspace root)
[workspace.lints.clippy]
all                           = "deny"
pedantic                      = "deny"
nursery                       = "deny"
cargo                         = "deny"
missing_docs_in_private_items = "deny"
missing_errors_doc            = "deny"
missing_panics_doc            = "deny"

[lints]
workspace = true
```

---

## 6. Code Standards

### 6.1 Error Handling

> [!CAUTION]
> **Never** use `.unwrap()` or `.expect()` in library/production code.
> Reserve them exclusively for tests and provably-infallible cases with a comment.

Use `thiserror` for library error types and `anyhow` for application-level error propagation.

```rust
use thiserror::Error;

/// Errors that can occur during data processing.
#[derive(Error, Debug)]
pub enum DataProcessingError {
    #[error("invalid data format: {0}")]
    InvalidFormat(String),

    #[error("processing timeout after {duration_secs}s")]
    Timeout { duration_secs: u64 },

    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("serialization error")]
    Serialization(#[from] serde_json::Error),
}

/// Process raw input into a structured record.
///
/// # Errors
///
/// Returns [`DataProcessingError::InvalidFormat`] if `input` cannot be parsed.
pub fn process_data(input: &str) -> Result<ProcessedData, DataProcessingError> {
    let parsed = parse_input(input)?;
    let processed = transform_data(parsed)?;
    Ok(processed)
}
```

**Error Handling Checklist:**

- [ ] Every error communicates **what**, **where**, and **why**.
- [ ] No silent failures — all `Result` values are propagated or logged.
- [ ] Errors are contextually wrapped at module boundaries.
- [ ] `#[from]` is used for transparent conversions; manual `map_err` for added context.

---

### 6.2 Async / Await Best Practices

```rust
use tokio::time::{timeout, Duration};

/// Fetch data from `url` with a timeout.
///
/// # Errors
///
/// Returns [`DataProcessingError::Timeout`] if the request exceeds `timeout_duration`.
/// Returns [`DataProcessingError::Io`] on network failure.
pub async fn fetch_data_with_timeout(
    url: &str,
    timeout_duration: Duration,
) -> Result<Data, DataProcessingError> {
    let client = reqwest::Client::new();

    let response = timeout(timeout_duration, client.get(url).send())
        .await
        .map_err(|_| DataProcessingError::Timeout {
            duration_secs: timeout_duration.as_secs(),
        })?
        .map_err(|e| DataProcessingError::Io(e.into()))?;

    let data: Data = response
        .json()
        .await
        .map_err(DataProcessingError::Serialization)?;

    Ok(data)
}
```

**Async Rules:**

- Prefer `tokio` as the async runtime for all server-side work.
- Always set timeouts on external I/O (network, file, IPC).
- Use `tokio::select!` for concurrent branch cancellation, **not** manual `JoinHandle` polling.
- Avoid `block_on` inside async contexts — it will deadlock the runtime.
- Use structured concurrency (`JoinSet`, `TaskTracker`) over raw `tokio::spawn`.

---

### 6.3 Memory Management & Ownership

```rust
use std::collections::HashMap;
use std::sync::Arc;

/// A data processor with an internal cache.
pub struct DataProcessor {
    cache: Arc<HashMap<String, ProcessedData>>,
    config: Arc<ProcessorConfig>,
}

impl DataProcessor {
    /// Create a new processor with the given configuration.
    pub fn new(config: ProcessorConfig) -> Self {
        Self {
            cache: Arc::new(HashMap::new()),
            config: Arc::new(config),
        }
    }

    /// Process `input`, returning a cached result if available.
    ///
    /// # Errors
    ///
    /// Returns a [`DataProcessingError`] if processing fails.
    pub async fn process(
        &self,
        input: &str,
    ) -> Result<ProcessedData, DataProcessingError> {
        if let Some(cached) = self.cache.get(input) {
            return Ok(cached.clone());
        }

        let result = self.process_internal(input).await?;
        // In production, use RwLock<HashMap> or dashmap for mutable caching.
        Ok(result)
    }
}
```

**Ownership Rules:**

- Prefer borrowing (`&T`, `&mut T`) over cloning.
- Use `Arc` only when shared ownership across threads is required; prefer `Rc` in single-threaded code.
- Use `Cow<'_, str>` when a function may or may not need to allocate.
- Avoid `Box<dyn Trait>` when generics (`impl Trait` or `<T: Trait>`) suffice.
- Use `Arc<[T]>` instead of `Arc<Vec<T>>` for immutable shared slices.

---

### 6.4 Type System & API Design

- **Newtype pattern**: Wrap primitive types to add semantic meaning and prevent misuse.
- **Builder pattern**: Use for structs with many optional fields (prefer `bon` or `typed-builder` crates).
- **`#[must_use]`**: Apply to functions whose return value should not be silently discarded.
- **`#[non_exhaustive]`**: Apply to public enums and structs that may grow.
- **Sealed traits**: Use the sealed-trait pattern for traits not intended for external implementation.

```rust
/// A validated, non-empty project name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectName(String);

impl ProjectName {
    /// Create a new project name.
    ///
    /// # Errors
    ///
    /// Returns an error if `name` is empty or contains invalid characters.
    pub fn new(name: impl Into<String>) -> Result<Self, ValidationError> {
        let name = name.into();
        if name.is_empty() {
            return Err(ValidationError::Empty("project name"));
        }
        Ok(Self(name))
    }
}
```

---

### 6.5 Documentation Standards

Every public item **must** have a doc comment (`///`) that includes:

1. **Summary** — one-line description.
2. **Details** — extended explanation (if needed).
3. **`# Errors`** — documents each error variant the function can return.
4. **`# Panics`** — documents conditions under which the function panics (ideally none).
5. **`# Examples`** — runnable code example (serves as a doc-test).

```rust
/// Compute the weighted average of `values` using `weights`.
///
/// Both slices must have the same length and contain at least one element.
///
/// # Errors
///
/// Returns [`StatsError::EmptyInput`] if either slice is empty.
/// Returns [`StatsError::LengthMismatch`] if the slices differ in length.
///
/// # Examples
///
/// ```
/// # use my_crate::weighted_average;
/// let avg = weighted_average(&[1.0, 2.0, 3.0], &[0.5, 0.3, 0.2])?;
/// assert!((avg - 1.7).abs() < f64::EPSILON);
/// # Ok::<(), my_crate::StatsError>(())
/// ```
pub fn weighted_average(
    values: &[f64],
    weights: &[f64],
) -> Result<f64, StatsError> {
    // ...
}
```

---

## 7. Testing Standards

### 7.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_valid_csv_returns_correct_count() {
        let processor = DataProcessor::new(ProcessorConfig::default());
        let input = "test,data,here";

        let result = processor.process(input).unwrap();

        assert_eq!(result.record_count, 3);
        assert!(result.is_valid());
    }

    #[test]
    fn process_invalid_format_returns_error() {
        let processor = DataProcessor::new(ProcessorConfig::default());

        let result = processor.process("invalid format");

        assert!(matches!(
            result,
            Err(DataProcessingError::InvalidFormat(_))
        ));
    }

    #[tokio::test]
    async fn async_process_returns_valid_data() {
        let processor = DataProcessor::new(ProcessorConfig::default());

        let result = processor.process("async,test,data").await.unwrap();

        assert_eq!(result.record_count, 3);
    }
}
```

### 7.2 Property-Based Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn process_never_panics(input in "[a-zA-Z0-9,]+") {
        let processor = DataProcessor::new(ProcessorConfig::default());
        let result = processor.process(&input);

        match result {
            Ok(data) => {
                prop_assert!(data.record_count >= 0);
                prop_assert!(data.is_valid());
            }
            Err(DataProcessingError::InvalidFormat(_)) => { /* acceptable */ }
            Err(e) => prop_assert!(false, "unexpected error: {e}"),
        }
    }
}
```

### 7.3 Testing Checklist

- [ ] Every public function has at least one happy-path and one error-path test.
- [ ] Async functions are tested with `#[tokio::test]`.
- [ ] Edge cases (empty input, max values, unicode) are covered.
- [ ] Property-based tests exist for complex transformations.
- [ ] Mocks (`mockall`) are used for external dependencies.
- [ ] Doc-tests compile and pass (`cargo test --doc`).

---

## 8. Performance Benchmarking

Use **Criterion** for all performance-sensitive code paths.

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_data_processing(c: &mut Criterion) {
    let processor = DataProcessor::new(ProcessorConfig::default());
    let data = "benchmark,test,data,with,multiple,records";

    c.bench_function("data_processing_sync", |b| {
        b.iter(|| processor.process(black_box(data)))
    });
}

fn bench_async_data_processing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let processor = DataProcessor::new(ProcessorConfig::default());
    let data = "async,benchmark,test,data";

    c.bench_function("data_processing_async", |b| {
        b.to_async(&rt)
            .iter(|| async { processor.process(black_box(data)).await })
    });
}

criterion_group!(benches, bench_data_processing, bench_async_data_processing);
criterion_main!(benches);
```

---

## 9. CI/CD Integration

```yaml
# .github/workflows/rust-ci.yml
name: Rust CI

on: [push, pull_request]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - uses: Swatinem/rust-cache@v2

      - name: Formatting
        run: cargo fmt --all -- --check

      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Tests
        run: cargo test --all-features --verbose

      - name: Doc Tests
        run: cargo test --doc

  coverage:
    runs-on: ubuntu-latest
    needs: check
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Install cargo-tarpaulin
        run: cargo install cargo-tarpaulin

      - name: Generate coverage
        run: cargo tarpaulin --out Html --output-dir coverage

      - name: Upload coverage
        uses: codecov/codecov-action@v4
        with:
          file: coverage/tarpaulin-report.html

  security:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Security audit
        uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
```

---

## 10. Tools & Technologies

### Development

| Tool | Purpose |
| :--- | :--- |
| `rustup` | Toolchain management |
| `rustfmt` | Code formatting |
| `clippy` | Linting & code analysis |
| `rust-analyzer` | Language server / IDE support |
| `cargo` | Build, test, package management |

### Testing & Benchmarking

| Tool | Purpose |
| :--- | :--- |
| `cargo test` | Built-in unit & integration tests |
| `criterion` | Statistical benchmarking |
| `proptest` | Property-based / fuzz testing |
| `mockall` | Mocking framework |
| `cargo-tarpaulin` | Code coverage |

### Quality & Security

| Tool | Purpose |
| :--- | :--- |
| `cargo audit` | Security vulnerability scanning |
| `cargo outdated` | Dependency staleness check |
| `cargo tree` | Dependency graph visualization |
| `cargo expand` | Macro expansion debugging |
| `cargo deny` | License & advisory policies |

---

## 11. Metrics & Monitoring

### Code Quality Metrics

| Metric | Target | Tool |
| :--- | :--- | :--- |
| Test coverage | ≥ 90% (100% public API) | `cargo-tarpaulin` |
| Clippy warnings | 0 | `cargo clippy` |
| Doc coverage | 100% public API | `cargo doc` |
| Benchmark regressions | < 5% | Criterion |

### Development Metrics

| Metric | Purpose |
| :--- | :--- |
| Build time | Track incremental & clean build perf |
| Dependency count | Minimize supply-chain surface |
| Security advisories | Zero unmitigated CVEs |

---

## 12. Review Cadence

| Frequency | Action |
| :--- | :--- |
| **Per PR** | Format, lint, test, code review |
| **Weekly** | Review code quality dashboard |
| **Monthly** | `cargo update`, `cargo audit`, `cargo outdated` |
| **Quarterly** | Review & update this standards document |
| **On release** | Evaluate new Rust stable features for adoption |

### Update Triggers

- **New Rust stable release** → Immediate evaluation; update `rust-version` if adopting.
- **Security advisory** → Immediate patching via `cargo audit fix`.
- **New tooling release** → Evaluate and adopt within the monthly cycle.
- **Performance regression** → Investigate via Criterion baselines.

---

## 13. Quick Reference – Prohibited Patterns

| ❌ Don't | ✅ Do Instead |
| :--- | :--- |
| `.unwrap()` in production | Use `?`, `map_err`, or `.unwrap_or_default()` |
| `println!` for logging | Use `tracing::info!` / `tracing::error!` |
| `clone()` without reason | Borrow first; clone only when ownership is needed |
| Raw `thread::spawn` | Use `tokio::spawn` with structured concurrency |
| `unsafe` without comment | Add `// SAFETY:` explaining the invariant |
| Magic numbers | Named constants or enums |
| Wildcard imports `use foo::*` | Explicit imports or re-exports |
| Mutable globals | `OnceLock`, DI, or runtime config |

---

> **Maintained by:** The Architect role (High-Reasoning Model)
> **Compliance:** All code contributions are validated against this document during the Reflect phase.
