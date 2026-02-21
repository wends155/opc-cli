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

### 5.2 Clippy Configuration

Use the workspace-level `[lints]` table (Rust 1.74+) for consistent configuration
across all crates. The default `clippy::all` group already covers correctness and
common-style lints — that is **sufficient as the baseline**. Layer `pedantic` as
**warnings** for guidance, and cherry-pick high-value individual lints.

> [!TIP]
> Avoid blanket `deny` on `nursery` (lints are unstable and may change between
> releases) or `cargo` (situational, better handled per-project). Promote
> individual lints to `deny` only when the team has validated they don't produce
> false positives in the codebase.

```toml
# Cargo.toml (workspace root)
[workspace.lints.clippy]
# ── Baseline (default) ────────────────────────────────────────
all = "deny"                          # correctness + common style

# ── Guidance ──────────────────────────────────────────────────
pedantic = "warn"                     # stricter style — warn, don't block

# ── Cherry-picked high-value lints (deny) ─────────────────────
missing_errors_doc       = "deny"     # every Result-returning fn must document errors
missing_panics_doc       = "deny"     # every potentially-panicking fn must document it
undocumented_unsafe_blocks = "deny"   # enforce // SAFETY: comments
cast_possible_truncation = "deny"     # catch lossy integer casts
large_futures            = "deny"     # prevent accidentally huge futures on the stack

# ── Useful pedantic lints relaxed to allow (override as needed) ─
module_name_repetitions  = "allow"    # common in domain-driven designs
must_use_candidate       = "allow"    # too noisy for general use

[workspace.lints.rust]
unsafe_code              = "warn"     # highlight unsafe usage without hard-blocking

[lints]
workspace = true
```

#### Recommended Per-Project Additions

Enable these when they apply to your project:

| Lint | Level | When to enable |
| :--- | :--- | :--- |
| `clippy::nursery` | `warn` | Opt-in for experimental early warnings |
| `clippy::cargo` | `warn` | When publishing crates to crates.io |
| `clippy::missing_docs_in_private_items` | `warn` | For library-heavy projects needing internal docs |
| `clippy::unwrap_used` | `deny` | For production services (not tests) |
| `clippy::expect_used` | `warn` | Pair with `unwrap_used` for stricter error handling |
| `clippy::indexing_slicing` | `warn` | For safety-critical code avoiding panics |

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
- **Builder pattern**: Use for structs with many optional fields (see § 6.6.1 for full guidance).
- **Typestate pattern**: Encode valid state transitions in the type system (see § 6.6.2).
- **`#[must_use]`**: Apply to functions whose return value should not be silently discarded.
- **`#[non_exhaustive]`**: Apply to public enums and structs that may grow.
- **Sealed traits**: Use the sealed-trait pattern for traits not intended for external implementation.
- See § 6.6 for the complete design patterns reference and selection guide.

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

### 6.6 Design Patterns & Best Practices

Rust's type system enables patterns that catch entire categories of bugs at compile time.
This section codifies the patterns we use most, with guidance on when and why to apply each.

#### 6.6.1 Builder Pattern

Use when constructing structs with many fields — especially when some are optional, have
defaults, or require validation. Prefer crate-based derive macros for boilerplate-free builders.

> [!TIP]
> Prefer the [`bon`](https://docs.rs/bon) or [`typed-builder`](https://docs.rs/typed-builder)
> crates. Fall back to a manual builder only when you need custom validation logic during construction.

**Derive approach (recommended):**

```rust
use bon::Builder;

/// Configuration for a data processing pipeline.
#[derive(Debug, Builder)]
pub struct PipelineConfig {
    /// Source connection string (required).
    source: String,
    /// Target connection string (required).
    target: String,
    /// Maximum records per batch.
    #[builder(default = 1000)]
    batch_size: usize,
    /// Enable compression on the wire.
    #[builder(default)]
    compress: bool,
    /// Optional timeout override.
    #[builder(default)]
    timeout: Option<Duration>,
}

// Usage — compile error if required fields are missing:
let config = PipelineConfig::builder()
    .source("postgres://localhost/src".into())
    .target("postgres://localhost/dst".into())
    .batch_size(5000)
    .build();
```

**Manual builder (when validation is needed):**

```rust
/// Builder for [`PipelineConfig`] with validation.
pub struct PipelineConfigBuilder {
    source: Option<String>,
    target: Option<String>,
    batch_size: usize,
}

impl PipelineConfigBuilder {
    pub fn new() -> Self {
        Self {
            source: None,
            target: None,
            batch_size: 1000,
        }
    }

    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Consume the builder and produce a validated [`PipelineConfig`].
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError`] if required fields are missing.
    pub fn build(self) -> Result<PipelineConfig, ValidationError> {
        let source = self.source.ok_or(ValidationError::MissingField("source"))?;
        let target = self.target.ok_or(ValidationError::MissingField("target"))?;

        if self.batch_size == 0 {
            return Err(ValidationError::InvalidValue("batch_size must be > 0"));
        }

        Ok(PipelineConfig {
            source,
            target,
            batch_size: self.batch_size,
            compress: false,
            timeout: None,
        })
    }
}
```

---

#### 6.6.2 Typestate Pattern

Encode protocol steps or lifecycle phases into the type system so that **invalid state
transitions are compile-time errors**. Each state is a zero-sized type used as a generic
parameter — no runtime cost.

```rust
use std::marker::PhantomData;

// ── State markers (zero-sized) ──────────────────────────────
pub struct Disconnected;
pub struct Connected;
pub struct Authenticated;

/// A database connection whose available operations depend on its state `S`.
pub struct Connection<S> {
    addr: String,
    _state: PhantomData<S>,
}

// ── Methods available only in Disconnected state ────────────
impl Connection<Disconnected> {
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            _state: PhantomData,
        }
    }

    /// Open a TCP connection. Transitions to `Connected`.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectionError::Tcp`] if the handshake fails.
    pub fn connect(self) -> Result<Connection<Connected>, ConnectionError> {
        // ... perform TCP handshake ...
        Ok(Connection {
            addr: self.addr,
            _state: PhantomData,
        })
    }
}

// ── Methods available only in Connected state ───────────────
impl Connection<Connected> {
    /// Authenticate with credentials. Transitions to `Authenticated`.
    pub fn authenticate(
        self,
        creds: &Credentials,
    ) -> Result<Connection<Authenticated>, ConnectionError> {
        // ... perform auth ...
        Ok(Connection {
            addr: self.addr,
            _state: PhantomData,
        })
    }
}

// ── Methods available only in Authenticated state ───────────
impl Connection<Authenticated> {
    /// Execute a query — only available after authentication.
    pub fn query(&self, sql: &str) -> Result<ResultSet, QueryError> {
        // ... run query ...
        todo!()
    }
}
```

> [!IMPORTANT]
> Calling `.query()` on a `Connection<Connected>` (unauthenticated) is a **compile error**.
> The state machine is enforced entirely by the compiler with zero runtime overhead.

**When to use:**
- Protocol handshakes (connect → auth → ready).
- Build pipelines (configure → validate → execute).
- File I/O (open → write → flush → close).

---

#### 6.6.3 RAII & Drop Guards

Use Rust's `Drop` trait to guarantee resource cleanup runs automatically when a value
goes out of scope — even on early returns or panics. This replaces `try/finally` blocks
from other languages.

```rust
/// A transaction guard that automatically rolls back unless explicitly committed.
pub struct TransactionGuard<'a> {
    conn: &'a mut Connection<Authenticated>,
    committed: bool,
}

impl<'a> TransactionGuard<'a> {
    /// Begin a new transaction.
    pub fn begin(conn: &'a mut Connection<Authenticated>) -> Result<Self, QueryError> {
        conn.execute("BEGIN")?;
        Ok(Self {
            conn,
            committed: false,
        })
    }

    /// Execute a statement within this transaction.
    pub fn execute(&mut self, sql: &str) -> Result<(), QueryError> {
        self.conn.execute(sql)
    }

    /// Commit the transaction. Prevents rollback on drop.
    pub fn commit(mut self) -> Result<(), QueryError> {
        self.conn.execute("COMMIT")?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for TransactionGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            // Best-effort rollback — log but don't panic.
            if let Err(e) = self.conn.execute("ROLLBACK") {
                tracing::error!(error = %e, "failed to rollback transaction on drop");
            }
        }
    }
}
```

**Common RAII use cases:**

| Resource | Guard / Type | Cleanup Action |
|:---|:---|:---|
| Database transaction | `TransactionGuard` | Rollback on drop |
| Temp file / directory | `tempfile::TempDir` | Delete on drop |
| Mutex lock | `MutexGuard` | Release on drop |
| Timer / span | `tracing::span::Entered` | Record elapsed on drop |
| File lock | `fs2::FileLock` | Release on drop |

> [!TIP]
> For ad-hoc guards without a dedicated struct, use the
> [`scopeguard`](https://docs.rs/scopeguard) crate:
> ```rust
> use scopeguard::defer;
> defer! { cleanup_temp_files(); }
> ```

---

#### 6.6.4 Extension Traits

Add domain-specific methods to types you don't own (e.g., `std`, `serde_json`) without
violating the orphan rule. Define a trait, implement it for the foreign type, and re-export it.

```rust
/// Extends [`Result`] with contextual error wrapping.
pub trait ResultExt<T, E> {
    /// Wrap the error with additional context.
    fn context(self, msg: &'static str) -> Result<T, ContextError<E>>;
}

impl<T, E: std::error::Error> ResultExt<T, E> for Result<T, E> {
    fn context(self, msg: &'static str) -> Result<T, ContextError<E>> {
        self.map_err(|e| ContextError {
            context: msg,
            source: e,
        })
    }
}

/// Extends `&str` with data-engineering helpers.
pub trait StrExt {
    /// Return `None` if the string is empty or whitespace-only.
    fn non_blank(&self) -> Option<&str>;
}

impl StrExt for str {
    fn non_blank(&self) -> Option<&str> {
        let trimmed = self.trim();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    }
}
```

**Rules:**
- Name the trait `<Type>Ext` (e.g., `StrExt`, `ResultExt`, `IteratorExt`).
- Keep extension traits in a dedicated `ext` module.
- Re-export from the crate prelude if they're used widely.

---

#### 6.6.5 Interior Mutability

Use interior mutability when you need to mutate data behind a shared reference (`&T`).
Choose the narrowest primitive that satisfies your requirements:

| Type | Thread-safe? | Checked at | Use when |
|:---|:---|:---|:---|
| `Cell<T>` | ❌ | Compile time | `T: Copy`, single thread, simple swap/replace |
| `RefCell<T>` | ❌ | Runtime | Single thread, need `&mut T` borrows |
| `Mutex<T>` | ✅ | Runtime | Multi-thread, exclusive write access |
| `RwLock<T>` | ✅ | Runtime | Multi-thread, many readers / rare writers |
| `OnceLock<T>` | ✅ | Runtime | Write-once lazy initialization |
| `Atomic*` | ✅ | Lock-free | Counters, flags, simple numeric state |

> [!CAUTION]
> Prefer `OnceLock` (std) or `once_cell::sync::Lazy` over hand-rolled `Mutex<Option<T>>`
> for lazy initialization. It's safer and more readable.

```rust
use std::sync::OnceLock;

/// Application-wide configuration, initialized once at startup.
static CONFIG: OnceLock<AppConfig> = OnceLock::new();

pub fn init_config(config: AppConfig) {
    CONFIG.set(config).expect("config already initialized");
}

/// Get the global config. Panics if [`init_config`] was not called.
pub fn config() -> &'static AppConfig {
    CONFIG.get().expect("config not initialized — call init_config() first")
}
```

---

#### 6.6.6 Enum Dispatch vs Trait Objects

Choose between compile-time (`enum`) and runtime (`dyn Trait`) polymorphism based on
whether the set of variants is **closed** or **open**.

| Criterion | Enum dispatch | Trait objects (`dyn Trait`) |
|:---|:---|:---|
| Variant set | Closed (known at compile time) | Open (extensible by consumers) |
| Performance | Monomorphized, inlineable | Vtable indirection |
| Pattern matching | ✅ Exhaustive `match` | ❌ Not available |
| Object safety needed? | No | Yes |
| Binary size | Larger (monomorphization) | Smaller |

**Enum dispatch (closed set, preferred when possible):**

```rust
/// All supported data formats — known at compile time.
#[non_exhaustive]
pub enum DataFormat {
    Csv(CsvHandler),
    Json(JsonHandler),
    Parquet(ParquetHandler),
}

impl DataFormat {
    /// Read records from `reader` in this format.
    pub fn read(&self, reader: &mut dyn Read) -> Result<Vec<Record>, FormatError> {
        match self {
            Self::Csv(h) => h.read(reader),
            Self::Json(h) => h.read(reader),
            Self::Parquet(h) => h.read(reader),
        }
    }
}
```

**Trait objects (open set, plugin-style extensibility):**

```rust
/// A data source that can be implemented by consumers.
pub trait DataSource: Send + Sync {
    /// Read all records from this source.
    fn read_all(&self) -> Result<Vec<Record>, SourceError>;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}

/// Accept any data source — open for extension.
pub fn ingest(sources: &[Box<dyn DataSource>]) -> Result<(), IngestError> {
    for source in sources {
        tracing::info!(source = source.name(), "ingesting");
        let records = source.read_all()?;
        store(records)?;
    }
    Ok(())
}
```

> [!TIP]
> When the set is closed but you're tempted by traits for ergonomics, consider the
> [`enum_dispatch`](https://docs.rs/enum_dispatch) crate — it auto-generates the
> `match` arms and gives you trait syntax with enum performance.

---

#### 6.6.7 Pattern Selection Guide

| Problem | Recommended Pattern | Ref |
|:---|:---|:---|
| Many optional constructor fields | Builder | § 6.6.1 |
| Compile-time state machine enforcement | Typestate | § 6.6.2 |
| Guaranteed resource cleanup | RAII / Drop Guard | § 6.6.3 |
| Add methods to types you don't own | Extension Trait | § 6.6.4 |
| Mutation behind `&T` | Interior Mutability | § 6.6.5 |
| Known, closed set of behaviors | Enum Dispatch | § 6.6.6 |
| Open, extensible set of behaviors | Trait Objects (`dyn`) | § 6.6.6 |
| Prevent primitive type misuse | Newtype | § 6.4 |
| Restrict external trait implementations | Sealed Trait | § 6.4 |

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

- [ ] Every function has at least one happy-path and one error-path test.
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
| Boolean flags for state tracking | Typestate pattern (§ 6.6.2) |
| Manual resource cleanup calls | RAII / Drop guard (§ 6.6.3) |
| `struct` with 10+ constructor args | Builder pattern (§ 6.6.1) |
| Wrapper structs for one method | Extension trait (§ 6.6.4) |
| `Mutex<Option<T>>` for lazy init | `OnceLock` or `LazyLock` (§ 6.6.5) |

---

> **Maintained by:** The Architect role (High-Reasoning Model)
> **Compliance:** All code contributions are validated against this document during the Reflect phase.
