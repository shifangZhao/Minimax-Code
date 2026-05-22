---
name: rust-patterns
description: Idiomatic Rust patterns, ownership, error handling, traits, concurrency, and best practices for building safe, performant applications.
origin: ECC
---

# Rust Development Patterns

Idiomatic Rust patterns and best practices for building safe, performant, and maintainable applications.

## When to Use

- Writing new Rust code
- Reviewing Rust code
- Refactoring existing Rust code
- Designing crate structure and module layout

## How It Works

This skill enforces idiomatic Rust conventions across six key areas: ownership and borrowing to prevent data races at compile time, `Result`/`?` error propagation with `thiserror` for libraries and `anyhow` for applications, enums and exhaustive pattern matching to make illegal states unrepresentable, traits and generics for zero-cost abstraction, safe concurrency via `Arc<Mutex<T>>`, channels, and async/await, and minimal `pub` surfaces organized by domain.

## Core Principles

### 1. Ownership and Borrowing

Rust's ownership system prevents data races and memory bugs at compile time.

```rust
// Good: Pass references when you don't need ownership
fn process(data: &[u8]) -> usize {
    data.len()
}

// Good: Take ownership only when you need to store or consume
fn store(data: Vec<u8>) -> Record {
    Record { payload: data }
}

// Bad: Cloning unnecessarily to avoid borrow checker
fn process_bad(data: &Vec<u8>) -> usize {
    let cloned = data.clone(); // Wasteful — just borrow
    cloned.len()
}
```

### Use `Cow` for Flexible Ownership

```rust
use std::borrow::Cow;

fn normalize(input: &str) -> Cow<'_, str> {
    if input.contains(' ') {
        Cow::Owned(input.replace(' ', "_"))
    } else {
        Cow::Borrowed(input) // Zero-cost when no mutation needed
    }
}
```

## Error Handling

### Use `Result` and `?` — Never `unwrap()` in Production

```rust
// Good: Propagate errors with context
use anyhow::{Context, Result};

fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config from {path}"))?;
    let config: Config = toml::from_str(&content)
        .with_context(|| format!("failed to parse config from {path}"))?;
    Ok(config)
}

// Bad: Panics on error
fn load_config_bad(path: &str) -> Config {
    let content = std::fs::read_to_string(path).unwrap(); // Panics!
    toml::from_str(&content).unwrap()
}
```

### Library Errors with `thiserror`, Application Errors with `anyhow`

```rust
// Library code: structured, typed errors
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("record not found: {id}")]
    NotFound { id: String },
    #[error("connection failed")]
    Connection(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    InvalidData(String),
}

// Application code: flexible error handling
use anyhow::{bail, Result};

fn run() -> Result<()> {
    let config = load_config("app.toml")?;
    if config.workers == 0 {
        bail!("worker count must be > 0");
    }
    Ok(())
}
```

### `Option` Combinators Over Nested Matching

```rust
// Good: Combinator chain
fn find_user_email(users: &[User], id: u64) -> Option<String> {
    users.iter()
        .find(|u| u.id == id)
        .map(|u| u.email.clone())
}

// Bad: Deeply nested matching
fn find_user_email_bad(users: &[User], id: u64) -> Option<String> {
    match users.iter().find(|u| u.id == id) {
        Some(user) => match &user.email {
            email => Some(email.clone()),
        },
        None => None,
    }
}
```

## Enums and Pattern Matching

### Model States as Enums

```rust
// Good: Impossible states are unrepresentable
enum ConnectionState {
    Disconnected,
    Connecting { attempt: u32 },
    Connected { session_id: String },
    Failed { reason: String, retries: u32 },
}

fn handle(state: &ConnectionState) {
    match state {
        ConnectionState::Disconnected => connect(),
        ConnectionState::Connecting { attempt } if *attempt > 3 => abort(),
        ConnectionState::Connecting { .. } => wait(),
        ConnectionState::Connected { session_id } => use_session(session_id),
        ConnectionState::Failed { retries, .. } if *retries < 5 => retry(),
        ConnectionState::Failed { reason, .. } => log_failure(reason),
    }
}
```

### Exhaustive Matching — No Catch-All for Business Logic

```rust
// Good: Handle every variant explicitly
match command {
    Command::Start => start_service(),
    Command::Stop => stop_service(),
    Command::Restart => restart_service(),
    // Adding a new variant forces handling here
}

// Bad: Wildcard hides new variants
match command {
    Command::Start => start_service(),
    _ => {} // Silently ignores Stop, Restart, and future variants
}
```

## Traits and Generics

### Accept Generics, Return Concrete Types

```rust
// Good: Generic input, concrete output
fn read_all(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

// Good: Trait bounds for multiple constraints
fn process<T: Display + Send + 'static>(item: T) -> String {
    format!("processed: {item}")
}
```

### Trait Objects for Dynamic Dispatch

```rust
// Use when you need heterogeneous collections or plugin systems
trait Handler: Send + Sync {
    fn handle(&self, request: &Request) -> Response;
}

struct Router {
    handlers: Vec<Box<dyn Handler>>,
}

// Use generics when you need performance (monomorphization)
fn fast_process<H: Handler>(handler: &H, request: &Request) -> Response {
    handler.handle(request)
}
```

### Newtype Pattern for Type Safety

```rust
// Good: Distinct types prevent mixing up arguments
struct UserId(u64);
struct OrderId(u64);

fn get_order(user: UserId, order: OrderId) -> Result<Order> {
    // Can't accidentally swap user and order IDs
    todo!()
}

// Bad: Easy to swap arguments
fn get_order_bad(user_id: u64, order_id: u64) -> Result<Order> {
    todo!()
}
```

## Structs and Data Modeling

### Builder Pattern for Complex Construction

```rust
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: usize,
}

impl ServerConfig {
    fn builder(host: impl Into<String>, port: u16) -> ServerConfigBuilder {
        ServerConfigBuilder { host: host.into(), port, max_connections: 100 }
    }
}

struct ServerConfigBuilder { host: String, port: u16, max_connections: usize }

impl ServerConfigBuilder {
    fn max_connections(mut self, n: usize) -> Self { self.max_connections = n; self }
    fn build(self) -> ServerConfig {
        ServerConfig { host: self.host, port: self.port, max_connections: self.max_connections }
    }
}

// Usage: ServerConfig::builder("localhost", 8080).max_connections(200).build()
```

## Iterators and Closures

### Prefer Iterator Chains Over Manual Loops

```rust
// Good: Declarative, lazy, composable
let active_emails: Vec<String> = users.iter()
    .filter(|u| u.is_active)
    .map(|u| u.email.clone())
    .collect();

// Bad: Imperative accumulation
let mut active_emails = Vec::new();
for user in &users {
    if user.is_active {
        active_emails.push(user.email.clone());
    }
}
```

### Use `collect()` with Type Annotation

```rust
// Collect into different types
let names: Vec<_> = items.iter().map(|i| &i.name).collect();
let lookup: HashMap<_, _> = items.iter().map(|i| (i.id, i)).collect();
let combined: String = parts.iter().copied().collect();

// Collect Results — short-circuits on first error
let parsed: Result<Vec<i32>, _> = strings.iter().map(|s| s.parse()).collect();
```

## Concurrency

### `Arc<Mutex<T>>` for Shared Mutable State

```rust
use std::sync::{Arc, Mutex};

let counter = Arc::new(Mutex::new(0));
let handles: Vec<_> = (0..10).map(|_| {
    let counter = Arc::clone(&counter);
    std::thread::spawn(move || {
        let mut num = counter.lock().expect("mutex poisoned");
        *num += 1;
    })
}).collect();

for handle in handles {
    handle.join().expect("worker thread panicked");
}
```

### Channels for Message Passing

```rust
use std::sync::mpsc;

let (tx, rx) = mpsc::sync_channel(16); // Bounded channel with backpressure

for i in 0..5 {
    let tx = tx.clone();
    std::thread::spawn(move || {
        tx.send(format!("message {i}")).expect("receiver disconnected");
    });
}
drop(tx); // Close sender so rx iterator terminates

for msg in rx {
    println!("{msg}");
}
```

### Async with Tokio

```rust
use tokio::time::Duration;

async fn fetch_with_timeout(url: &str) -> Result<String> {
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        reqwest::get(url),
    )
    .await
    .context("request timed out")?
    .context("request failed")?;

    response.text().await.context("failed to read body")
}

// Spawn concurrent tasks
async fn fetch_all(urls: Vec<String>) -> Vec<Result<String>> {
    let handles: Vec<_> = urls.into_iter()
        .map(|url| tokio::spawn(async move {
            fetch_with_timeout(&url).await
        }))
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(handle.await.unwrap_or_else(|e| panic!("spawned task panicked: {e}")));
    }
    results
}
```

## Unsafe Code

### When Unsafe Is Acceptable

```rust
// Acceptable: FFI boundary with documented invariants (Rust 2024+)
/// # Safety
/// `ptr` must be a valid, aligned pointer to an initialized `Widget`.
unsafe fn widget_from_raw<'a>(ptr: *const Widget) -> &'a Widget {
    // SAFETY: caller guarantees ptr is valid and aligned
    unsafe { &*ptr }
}

// Acceptable: Performance-critical path with proof of correctness
// SAFETY: index is always < len due to the loop bound
unsafe { slice.get_unchecked(index) }
```

### When Unsafe Is NOT Acceptable

```rust
// Bad: Using unsafe to bypass borrow checker
// Bad: Using unsafe for convenience
// Bad: Using unsafe without a Safety comment
// Bad: Transmuting between unrelated types
```

## Module System and Crate Structure

### Organize by Domain, Not by Type

```text
my_app/
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── auth/          # Domain module
│   │   ├── mod.rs
│   │   ├── token.rs
│   │   └── middleware.rs
│   ├── orders/        # Domain module
│   │   ├── mod.rs
│   │   ├── model.rs
│   │   └── service.rs
│   └── db/            # Infrastructure
│       ├── mod.rs
│       └── pool.rs
├── tests/             # Integration tests
├── benches/           # Benchmarks
└── Cargo.toml
```

### Visibility — Expose Minimally

```rust
// Good: pub(crate) for internal sharing
pub(crate) fn validate_input(input: &str) -> bool {
    !input.is_empty()
}

// Good: Re-export public API from lib.rs
pub mod auth;
pub use auth::AuthMiddleware;

// Bad: Making everything pub
pub fn internal_helper() {} // Should be pub(crate) or private
```

## Tooling Integration

### Essential Commands

```bash
# Build and check
cargo build
cargo check              # Fast type checking without codegen
cargo clippy             # Lints and suggestions
cargo fmt                # Format code

# Testing
cargo test
cargo test -- --nocapture    # Show println output
cargo test --lib             # Unit tests only
cargo test --test integration # Integration tests only

# Dependencies
cargo audit              # Security audit
cargo tree               # Dependency tree
cargo update             # Update dependencies

# Performance
cargo bench              # Run benchmarks
```

## Quick Reference: Rust Idioms

| Idiom | Description |
|-------|-------------|
| Borrow, don't clone | Pass `&T` instead of cloning unless ownership is needed |
| Make illegal states unrepresentable | Use enums to model valid states only |
| `?` over `unwrap()` | Propagate errors, never panic in library/production code |
| Parse, don't validate | Convert unstructured data to typed structs at the boundary |
| Newtype for type safety | Wrap primitives in newtypes to prevent argument swaps |
| Prefer iterators over loops | Declarative chains are clearer and often faster |
| `#[must_use]` on Results | Ensure callers handle return values |
| `Cow` for flexible ownership | Avoid allocations when borrowing suffices |
| Exhaustive matching | No wildcard `_` for business-critical enums |
| Minimal `pub` surface | Use `pub(crate)` for internal APIs |

## Anti-Patterns to Avoid

```rust
// Bad: .unwrap() in production code
let value = map.get("key").unwrap();

// Bad: .clone() to satisfy borrow checker without understanding why
let data = expensive_data.clone();
process(&original, &data);

// Bad: Using String when &str suffices
fn greet(name: String) { /* should be &str */ }

// Bad: Box<dyn Error> in libraries (use thiserror instead)
fn parse(input: &str) -> Result<Data, Box<dyn std::error::Error>> { todo!() }

// Bad: Ignoring must_use warnings
let _ = validate(input); // Silently discarding a Result

// Bad: Blocking in async context
async fn bad_async() {
    std::thread::sleep(Duration::from_secs(1)); // Blocks the executor!
    // Use: tokio::time::sleep(Duration::from_secs(1)).await;
}
```

**Remember**: If it compiles, it's probably correct — but only if you avoid `unwrap()`, minimize `unsafe`, and let the type system work for you.


---

---
name: rust-testing
description: Rust testing patterns including unit tests, integration tests, async testing, property-based testing, mocking, and coverage. Follows TDD methodology.
origin: ECC
---

# Rust Testing Patterns

Comprehensive Rust testing patterns for writing reliable, maintainable tests following TDD methodology.

## When to Use

- Writing new Rust functions, methods, or traits
- Adding test coverage to existing code
- Creating benchmarks for performance-critical code
- Implementing property-based tests for input validation
- Following TDD workflow in Rust projects

## How It Works

1. **Identify target code** — Find the function, trait, or module to test
2. **Write a test** — Use `#[test]` in a `#[cfg(test)]` module, rstest for parameterized tests, or proptest for property-based tests
3. **Mock dependencies** — Use mockall to isolate the unit under test
4. **Run tests (RED)** — Verify the test fails with the expected error
5. **Implement (GREEN)** — Write minimal code to pass
6. **Refactor** — Improve while keeping tests green
7. **Check coverage** — Use cargo-llvm-cov, target 80%+

## TDD Workflow for Rust

### The RED-GREEN-REFACTOR Cycle

```
RED     → Write a failing test first
GREEN   → Write minimal code to pass the test
REFACTOR → Improve code while keeping tests green
REPEAT  → Continue with next requirement
```

### Step-by-Step TDD in Rust

```rust
// RED: Write test first, use todo!() as placeholder
pub fn add(a: i32, b: i32) -> i32 { todo!() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_add() { assert_eq!(add(2, 3), 5); }
}
// cargo test → panics at 'not yet implemented'
```

```rust
// GREEN: Replace todo!() with minimal implementation
pub fn add(a: i32, b: i32) -> i32 { a + b }
// cargo test → PASS, then REFACTOR while keeping tests green
```

## Unit Tests

### Module-Level Test Organization

```rust
// src/user.rs
pub struct User {
    pub name: String,
    pub email: String,
}

impl User {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Result<Self, String> {
        let email = email.into();
        if !email.contains('@') {
            return Err(format!("invalid email: {email}"));
        }
        Ok(Self { name: name.into(), email })
    }

    pub fn display_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_user_with_valid_email() {
        let user = User::new("Alice", "alice@example.com").unwrap();
        assert_eq!(user.display_name(), "Alice");
        assert_eq!(user.email, "alice@example.com");
    }

    #[test]
    fn rejects_invalid_email() {
        let result = User::new("Bob", "not-an-email");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid email"));
    }
}
```

### Assertion Macros

```rust
assert_eq!(2 + 2, 4);                                    // Equality
assert_ne!(2 + 2, 5);                                    // Inequality
assert!(vec![1, 2, 3].contains(&2));                     // Boolean
assert_eq!(value, 42, "expected 42 but got {value}");    // Custom message
assert!((0.1_f64 + 0.2 - 0.3).abs() < f64::EPSILON);   // Float comparison
```

## Error and Panic Testing

### Testing `Result` Returns

```rust
#[test]
fn parse_returns_error_for_invalid_input() {
    let result = parse_config("}{invalid");
    assert!(result.is_err());

    // Assert specific error variant
    let err = result.unwrap_err();
    assert!(matches!(err, ConfigError::ParseError(_)));
}

#[test]
fn parse_succeeds_for_valid_input() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_config(r#"{"port": 8080}"#)?;
    assert_eq!(config.port, 8080);
    Ok(()) // Test fails if any ? returns Err
}
```

### Testing Panics

```rust
#[test]
#[should_panic]
fn panics_on_empty_input() {
    process(&[]);
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn panics_with_specific_message() {
    let v: Vec<i32> = vec![];
    let _ = v[0];
}
```

## Integration Tests

### File Structure

```text
my_crate/
├── src/
│   └── lib.rs
├── tests/              # Integration tests
│   ├── api_test.rs     # Each file is a separate test binary
│   ├── db_test.rs
│   └── common/         # Shared test utilities
│       └── mod.rs
```

### Writing Integration Tests

```rust
// tests/api_test.rs
use my_crate::{App, Config};

#[test]
fn full_request_lifecycle() {
    let config = Config::test_default();
    let app = App::new(config);

    let response = app.handle_request("/health");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, "OK");
}
```

## Async Tests

### With Tokio

```rust
#[tokio::test]
async fn fetches_data_successfully() {
    let client = TestClient::new().await;
    let result = client.get("/data").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().items.len(), 3);
}

#[tokio::test]
async fn handles_timeout() {
    use std::time::Duration;
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        slow_operation(),
    ).await;

    assert!(result.is_err(), "should have timed out");
}
```

## Test Organization Patterns

### Parameterized Tests with `rstest`

```rust
use rstest::{rstest, fixture};

#[rstest]
#[case("hello", 5)]
#[case("", 0)]
#[case("rust", 4)]
fn test_string_length(#[case] input: &str, #[case] expected: usize) {
    assert_eq!(input.len(), expected);
}

// Fixtures
#[fixture]
fn test_db() -> TestDb {
    TestDb::new_in_memory()
}

#[rstest]
fn test_insert(test_db: TestDb) {
    test_db.insert("key", "value");
    assert_eq!(test_db.get("key"), Some("value".into()));
}
```

### Test Helpers

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a test user with sensible defaults.
    fn make_user(name: &str) -> User {
        User::new(name, &format!("{name}@test.com")).unwrap()
    }

    #[test]
    fn user_display() {
        let user = make_user("alice");
        assert_eq!(user.display_name(), "alice");
    }
}
```

## Property-Based Testing with `proptest`

### Basic Property Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn encode_decode_roundtrip(input in ".*") {
        let encoded = encode(&input);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(input, decoded);
    }

    #[test]
    fn sort_preserves_length(mut vec in prop::collection::vec(any::<i32>(), 0..100)) {
        let original_len = vec.len();
        vec.sort();
        assert_eq!(vec.len(), original_len);
    }

    #[test]
    fn sort_produces_ordered_output(mut vec in prop::collection::vec(any::<i32>(), 0..100)) {
        vec.sort();
        for window in vec.windows(2) {
            assert!(window[0] <= window[1]);
        }
    }
}
```

### Custom Strategies

```rust
use proptest::prelude::*;

fn valid_email() -> impl Strategy<Value = String> {
    ("[a-z]{1,10}", "[a-z]{1,5}")
        .prop_map(|(user, domain)| format!("{user}@{domain}.com"))
}

proptest! {
    #[test]
    fn accepts_valid_emails(email in valid_email()) {
        assert!(User::new("Test", &email).is_ok());
    }
}
```

## Mocking with `mockall`

### Trait-Based Mocking

```rust
use mockall::{automock, predicate::eq};

#[automock]
trait UserRepository {
    fn find_by_id(&self, id: u64) -> Option<User>;
    fn save(&self, user: &User) -> Result<(), StorageError>;
}

#[test]
fn service_returns_user_when_found() {
    let mut mock = MockUserRepository::new();
    mock.expect_find_by_id()
        .with(eq(42))
        .times(1)
        .returning(|_| Some(User { id: 42, name: "Alice".into() }));

    let service = UserService::new(Box::new(mock));
    let user = service.get_user(42).unwrap();
    assert_eq!(user.name, "Alice");
}

#[test]
fn service_returns_none_when_not_found() {
    let mut mock = MockUserRepository::new();
    mock.expect_find_by_id()
        .returning(|_| None);

    let service = UserService::new(Box::new(mock));
    assert!(service.get_user(99).is_none());
}
```

## Doc Tests

### Executable Documentation

```rust
/// Adds two numbers together.
///
/// # Examples
///
/// ```
/// use my_crate::add;
///
/// assert_eq!(add(2, 3), 5);
/// assert_eq!(add(-1, 1), 0);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Parses a config string.
///
/// # Errors
///
/// Returns `Err` if the input is not valid TOML.
///
/// ```no_run
/// use my_crate::parse_config;
///
/// let config = parse_config(r#"port = 8080"#).unwrap();
/// assert_eq!(config.port, 8080);
/// ```
///
/// ```no_run
/// use my_crate::parse_config;
///
/// assert!(parse_config("}{invalid").is_err());
/// ```
pub fn parse_config(input: &str) -> Result<Config, ParseError> {
    todo!()
}
```

## Benchmarking with Criterion

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "benchmark"
harness = false
```

```rust
// benches/benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn fibonacci(n: u64) -> u64 {
    match n {
        0 | 1 => n,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn bench_fibonacci(c: &mut Criterion) {
    c.bench_function("fib 20", |b| b.iter(|| fibonacci(black_box(20))));
}

criterion_group!(benches, bench_fibonacci);
criterion_main!(benches);
```

## Test Coverage

### Running Coverage

```bash
# Install: cargo install cargo-llvm-cov (or use taiki-e/install-action in CI)
cargo llvm-cov                    # Summary
cargo llvm-cov --html             # HTML report
cargo llvm-cov --lcov > lcov.info # LCOV format for CI
cargo llvm-cov --fail-under-lines 80  # Fail if below threshold
```

### Coverage Targets

| Code Type | Target |
|-----------|--------|
| Critical business logic | 100% |
| Public API | 90%+ |
| General code | 80%+ |
| Generated / FFI bindings | Exclude |

## Testing Commands

```bash
cargo test                        # Run all tests
cargo test -- --nocapture         # Show println output
cargo test test_name              # Run tests matching pattern
cargo test --lib                  # Unit tests only
cargo test --test api_test        # Integration tests only
cargo test --doc                  # Doc tests only
cargo test --no-fail-fast         # Don't stop on first failure
cargo test -- --ignored           # Run ignored tests
```

## Best Practices

**DO:**
- Write tests FIRST (TDD)
- Use `#[cfg(test)]` modules for unit tests
- Test behavior, not implementation
- Use descriptive test names that explain the scenario
- Prefer `assert_eq!` over `assert!` for better error messages
- Use `?` in tests that return `Result` for cleaner error output
- Keep tests independent — no shared mutable state

**DON'T:**
- Use `#[should_panic]` when you can test `Result::is_err()` instead
- Mock everything — prefer integration tests when feasible
- Ignore flaky tests — fix or quarantine them
- Use `sleep()` in tests — use channels, barriers, or `tokio::time::pause()`
- Skip error path testing

## CI Integration

```yaml
# GitHub Actions
test:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: clippy, rustfmt

    - name: Check formatting
      run: cargo fmt --check

    - name: Clippy
      run: cargo clippy -- -D warnings

    - name: Run tests
      run: cargo test

    - uses: taiki-e/install-action@cargo-llvm-cov

    - name: Coverage
      run: cargo llvm-cov --fail-under-lines 80
```

**Remember**: Tests are documentation. They show how your code is meant to be used. Write them clearly and keep them up to date.
