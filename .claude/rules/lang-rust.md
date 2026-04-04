---
paths:
  - "**/*.rs"
---

# Rust

Rust enforces memory and thread safety at compile time.
Embrace the type system and borrow checker as design
tools, not obstacles.

## Ownership, Borrowing, and Lifetimes

### Ownership Design

- Prefer owned types for public API boundaries — this
  avoids lifetime complexity at interface boundaries
- Use references (`&T`, `&mut T`) to avoid unnecessary
  clones
- Keep lifetimes explicit only when the compiler requires it
- Use `Arc` or `Rc` for shared ownership scenarios
- Use `Cow<'_, T>` for data that may or may not be owned

### Red Flags

- **Excessive `.clone()`** indicates poor ownership design;
  restructure data flow instead
- **Fighting the borrow checker** means the design needs
  rethinking, not workarounds
- **Using `String` when `&str` suffices** creates
  unnecessary allocations

```rust
// Bad - unnecessary cloning
fn process(data: &MyStruct) -> String {
    let owned = data.clone();
    owned.name
}

// Good - borrow what you need
fn process(data: &MyStruct) -> &str {
    &data.name
}
```

## Type System

### Newtypes for Domain Concepts

Avoid primitive obsession — newtypes encode domain meaning
and validation at the type level, preventing mix-ups the
compiler catches:

```rust
#[derive(Debug, Clone, PartialEq)]
struct CustomerId(i64);

impl CustomerId {
    fn new(value: i64) -> Result<Self, ValidationError> {
        if value <= 0 {
            return Err(ValidationError::NonPositiveId);
        }
        Ok(Self(value))
    }
}
```

### Enums for State Machines

Use enums to make invalid states unrepresentable — the
compiler enforces exhaustive matching, so adding a new
variant surfaces every location that needs updating:

```rust
enum OrderStatus {
    Pending { created_at: DateTime<Utc> },
    Confirmed { confirmed_at: DateTime<Utc> },
    Shipped { tracking: TrackingNumber },
    Delivered { delivered_at: DateTime<Utc> },
}
```

### Enums Over Boolean Parameters

Replace boolean parameters with enums — boolean arguments
at call sites are unreadable and easy to swap accidentally:

```rust
// Unclear — what does `true` mean at the call site?
fn connect(host: &str, secure: bool) { /* ... */ }
connect("example.com", true);

// Explicit — intent is self-documenting and swap-proof
enum ConnectionMode { Secure, Plaintext }
fn connect(host: &str, mode: ConnectionMode) { /* ... */ }
connect("example.com", ConnectionMode::Secure);
```

### Exhaustive Struct Initialization

Avoid `..Default::default()` when constructing a value for
the first time — it silently ignores new fields, hiding
every place that needs updating:

```rust
// Fragile — new Config fields are silently defaulted
let config = Config {
    host: "localhost".to_string(),
    ..Default::default()
};

// Robust — compiler warns when Config gains a new field
let config = Config {
    host: "localhost".to_string(),
    port: 5432,
    timeout: Duration::from_secs(30),
};
```

Struct update syntax is fine when copying from an existing instance.

### Result and Option

- Use `Result<T, E>` for operations that can fail
- Use `Option<T>` for values that may be absent
- Chain with `.map()`, `.and_then()`, `.or_else()`
- Prefer `?` operator for clean error propagation

```rust
fn find_and_validate(
    id: CustomerId,
    repo: &dyn Repository,
) -> Result<ValidatedOrder, AppError> {
    repo.find(id)?
        .ok_or(AppError::NotFound(id))?
        .validate()
}
```

## Code Style and Tooling

### Required Tools

- `cargo fmt` before every commit (consistent formatting)
- `cargo clippy` with zero warnings — clippy catches
  correctness and performance issues the compiler misses
- `cargo test` must pass
- `cargo clean` before quality checks if stale incremental
  state is suspected — stale artifacts can hide errors

### Style Guidelines

- Functions under 50 lines; prefer early returns
- Limit nesting depth
- Meaningful names; avoid abbreviations
- Comments explain "why", not "what"

### Module Organization

- Use `<module>.rs` files, NOT `mod.rs` in `src/` — `mod.rs`
  hides the module name in editor tabs
  - For submodules: `domain.rs` with `domain/models.rs`
  - Exception: `mod.rs` is acceptable in `tests/`
- Organize by feature/domain, not by technical layer
- Re-export public APIs with `pub use`
- Use `snake_case` for file and folder names

## Recommended Crates

| Category | Crate | Purpose |
|---|---|---|
| Error handling | `thiserror` | Library error types |
| Error handling | `anyhow` | Application errors |
| Async | `tokio` | Async runtime |
| Async | `futures` | Future combinators |
| Serialization | `serde` | Serialize/deserialize |
| Logging | `tracing` | Structured logging |
| Testing | `proptest` | Property-based tests |
| Testing | `mockall` | Mock trait impls |
| Testing | `insta` | Snapshot testing |
| Testing | `test-case` | Parameterized test cases |
| Testing | `tokio-test` | Async I/O and task mocking |
| Benchmarking | `criterion` | Statistical benchmarks |
| Profiling | `flamegraph` | Flamegraph generation |
| Security | `secrecy` | Sensitive data |
| Collections | `im` | Immutable collections |

## Common Pitfalls

| Pitfall | Why It's Bad | Fix |
|---|---|---|
| Excessive `.clone()` | Poor ownership design | Restructure data flow |
| `unwrap()` in prod | Panics in production | Use `Result` and `?` |
| `String` vs `&str` | Unnecessary allocation | Borrow when possible |
| Deep trait hierarchies | Over-engineering | Composition over inheritance |
| Manual loops | Higher code mass | Use iterator chains |
| Complex generics | Hard to read | Simplify bounds |
| Premature `unsafe` | Undermines safety | Profile first |
| Ignoring warnings | Hides design issues | Fix all clippy warnings |
| Boolean parameters | Unreadable at call sites | Replace with enums |
| `..Default::default()` in construction | Silently hides new fields | Initialize all fields explicitly |
| Direct indexing (`items[0]`) | Panics at runtime | Use slice patterns |
