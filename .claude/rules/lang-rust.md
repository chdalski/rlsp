---
paths:
  - "**/*.rs"
---

# Rust

Rust prioritizes safety, performance, and expressiveness.
The compiler enforces memory safety and thread safety at
compile time. Embrace the type system and borrow checker
as design tools, not obstacles. Rust's ownership model
naturally aligns with functional programming and
immutability-first design.

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

Avoid `..Default::default()` or struct update syntax when
constructing a value for the first time — it silently
ignores new fields as they are added, hiding every place
that needs updating:

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

Exception: struct update syntax is appropriate when the
intent is genuinely "copy all other fields from an existing
instance" (e.g., `let updated = Config { port: 9000, ..existing }`).

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

## Error Handling

### Crate Choices

- `thiserror` for library/domain error type definitions
- `anyhow` for application-level error handling
- Never use `unwrap()` or `expect()` in production code —
  they panic and crash the process

### Custom Error Types

Define specific error types per module or domain — this
enables callers to handle different failures differently:

```rust
#[derive(Debug, thiserror::Error)]
enum RepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Entity not found: {0}")]
    NotFound(String),
}

#[derive(Debug, thiserror::Error)]
enum ValidationError {
    #[error("ID must be positive")]
    NonPositiveId,
    #[error("Name cannot be blank")]
    BlankName,
}
```

### Error Propagation

```rust
// Use ? operator, not unwrap
async fn fetch_order(
    id: OrderId,
    db: &Pool,
) -> Result<Order, AppError> {
    let row = sqlx::query("SELECT ...")
        .bind(id.0)
        .fetch_one(db)
        .await?;
    Order::try_from(row).map_err(AppError::Parse)
}
```

## Functional Patterns

### Slice Patterns Over Indexing

Use slice patterns instead of direct indexing — indexing
panics at runtime if the length assumption is wrong, while
patterns force explicit handling of every case at compile
time:

```rust
// Panics at runtime if items is empty or too short
let first = items[0];
let second = items[1];

// Compiler-enforced — all lengths must be handled
match items.as_slice() {
    [] => handle_empty(),
    [only] => handle_one(only),
    [first, second] => handle_two(first, second),
    [first, rest @ ..] => handle_many(first, rest),
}
```

### Iterator Chains Over Loops

Use iterator chains instead of imperative loops when the
criteria in `functional-style.md` are met (readability,
less code, no manual index math, lower complexity). The
compiler optimizes iterator chains to match hand-written
loop performance, so there is no runtime cost.

**Collect-and-push** — the most common anti-pattern. A
`mut Vec` is created, a `for` loop pushes items
conditionally. Always refactor:

```rust
// Anti-pattern — mutable accumulator + loop
let mut results = Vec::new();
for item in items {
    if item.is_valid() {
        results.push(item.transform());
    }
}

// Refactored — declarative, no mutation
let results: Vec<_> = items
    .iter()
    .filter(|item| item.is_valid())
    .map(|item| item.transform())
    .collect();
```

**Linear and reverse search** — loops that scan for a
match. Use `.find()`, `.position()`, or `.rev()` variants:

```rust
// Anti-pattern — manual reverse search with index math
for i in (0..current_line).rev() {
    if lines[i].indent() < current_indent {
        return Some(i);
    }
}

// Refactored — no index math, no off-by-one risk
lines[..current_line]
    .iter()
    .enumerate()
    .rev()
    .find(|(_, line)| line.indent() < current_indent)
    .map(|(i, _)| i)
```

**When loops are correct in Rust:**

- **`char_indices()` state machines** — tracking quote
  depth, nesting level, or parser state through `match`
  arms. A `.scan()` with a tuple accumulator is harder
  to read.
- **Async loops with multiple `.await` points** — a `for`
  loop with `await` and `?` is clearer than `Stream`
  combinators that require `StreamExt`, `TryStreamExt`,
  `Box::pin`, and lifetime annotations.
- **Recursive tree walks** — the recursive helper is
  inherently imperative. Forcing it into iterators
  requires a stack-based adapter that obscures traversal
  logic.

See `functional-style.md` for the full decision criteria.

### Immutability by Default

`let` bindings are immutable by default — use `mut` only
when needed. Expression-based language reduces need for
mutable accumulators:

```rust
// Mutation (higher mass)
let mut total = 0;
for price in prices {
    total += price;
}

// Functional (lower mass)
let total: i64 = prices.iter().sum();
```

### Function Composition

Build complex operations from small, composable functions —
each step is independently testable:

```rust
fn validate(req: Request) -> Result<Request, Error> {
    // ...
}
fn enrich(req: Request) -> EnrichedRequest { /* ... */ }
fn execute(req: EnrichedRequest) -> Result<Response, Error> {
    // ...
}

// Composed pipeline
fn process(req: Request) -> Result<Response, Error> {
    let validated = validate(req)?;
    let enriched = enrich(validated);
    execute(enriched)
}
```

### Higher-Order Functions

```rust
fn transform_dates<F>(
    dates: &[NaiveDate],
    transform: F,
) -> Vec<NaiveDate>
where
    F: Fn(&NaiveDate) -> NaiveDate,
{
    dates.iter().map(transform).collect()
}

let shifted = transform_dates(&dates, |d| {
    *d + Duration::days(7)
});
```

## Domain-Driven Design

### Value Objects as Newtypes

```rust
#[derive(Debug, Clone, PartialEq)]
struct Email(String);

impl Email {
    fn new(value: String) -> Result<Self, ValidationError> {
        if !value.contains('@') {
            return Err(ValidationError::InvalidEmail);
        }
        Ok(Self(value))
    }
}
```

### Aggregates with Invariant Enforcement

```rust
struct Order {
    id: OrderId,
    items: Vec<OrderItem>,
    status: OrderStatus,
}

impl Order {
    fn add_item(
        &mut self,
        item: OrderItem,
    ) -> Result<(), DomainError> {
        if self.status != OrderStatus::Draft {
            return Err(DomainError::OrderNotEditable);
        }
        self.items.push(item);
        Ok(())
    }
}
```

### Repository Traits

```rust
#[async_trait]
trait OrderRepository {
    async fn find(
        &self,
        id: OrderId,
    ) -> Result<Option<Order>, RepositoryError>;

    async fn save(
        &self,
        order: &Order,
    ) -> Result<(), RepositoryError>;
}
```

## Async Patterns

### Tokio Runtime

- Use `tokio` as the async runtime consistently
- Avoid blocking operations in async code — they stall the
  entire executor thread
- Use `tokio::spawn` for concurrent tasks
- Use channels for inter-task communication

```rust
async fn process_batch(
    items: Vec<Item>,
    client: &HttpClient,
) -> Vec<Result<Response, Error>> {
    let futures: Vec<_> = items
        .into_iter()
        .map(|item| client.process(item))
        .collect();
    futures::future::join_all(futures).await
}
```

## Testing

### Framework and Tools

- `cargo test` for unit and integration tests
- `proptest` for property-based testing
- `mockall` for mocking trait implementations

### Test Organization

Rust has three test locations, each with a distinct purpose
— choosing the right one keeps tests focused and avoids
over-mocking:

- **Inline `#[cfg(test)]` modules** — unit tests inside the
  source file; they have access to private items, which is
  the only way to test internal invariants directly
- **`tests/` directory** — integration tests compiled as a
  separate crate; they can only access public API, which
  makes them true black-box regression tests
- **Doc tests (`///`)** — code examples in documentation
  comments that `cargo test` runs as tests; use them for
  happy-path demonstrations so documentation and behaviour
  stay in sync

```rust
/// Returns a validated customer ID.
///
/// ```
/// # use mylib::CustomerId;
/// let id = CustomerId::new(42).unwrap();
/// assert_eq!(id.value(), 42);
/// ```
pub fn new(value: i64) -> Result<Self, ValidationError> { ... }
```

### Design for Testability

Keep business logic free of direct I/O — functions that call
`println!` embed an effect that tests cannot observe or
redirect without capturing stdout. Two complementary
patterns eliminate this:

**Accept `impl Write` for output** so tests pass a
`Vec<u8>` as an in-memory sink:

```rust
// Hard to test — output is embedded
fn print_report(orders: &[Order]) {
    for o in orders {
        println!("{}: {}", o.id, o.status);
    }
}

// Testable — caller injects the sink
fn write_report(
    orders: &[Order],
    out: &mut impl Write,
) -> io::Result<()> {
    for o in orders {
        writeln!(out, "{}: {}", o.id, o.status)?;
    }
    Ok(())
}

// In tests
let mut buf = Vec::new();
write_report(&orders, &mut buf)?;
assert_eq!(String::from_utf8(buf)?, "1: pending\n");
```

**Return action enums for decisions** so the caller executes
the effect and tests verify only the decision:

```rust
enum PricingDecision {
    ApplyDiscount { percent: u8 },
    NoDiscount,
}

fn evaluate_order(order: &Order) -> PricingDecision {
    if order.total > 100 {
        PricingDecision::ApplyDiscount { percent: 10 }
    } else {
        PricingDecision::NoDiscount
    }
}

#[test]
fn high_value_order_gets_discount() {
    let order = Order::with_total(150);
    assert!(matches!(
        evaluate_order(&order),
        PricingDecision::ApplyDiscount { percent: 10 }
    ));
}
```

### Test Structure

Use descriptive names with Arrange-Act-Assert pattern:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_id_rejects_zero_value() {
        let result = CustomerId::new(0);

        assert!(matches!(
            result,
            Err(ValidationError::NonPositiveId)
        ));
    }

    #[test]
    fn order_prevents_adding_items_when_confirmed() {
        let mut order = Order::confirmed(sample_id());

        let result = order.add_item(sample_item());

        assert!(matches!(
            result,
            Err(DomainError::OrderNotEditable)
        ));
    }
}
```

### Property-Based Testing

Use proptest to verify properties over random inputs — it
finds edge cases that manual test data misses:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn customer_id_always_positive(id in 1..i64::MAX) {
        let result = CustomerId::new(id);
        assert!(result.is_ok());
    }

    #[test]
    fn customer_id_rejects_non_positive(id in i64::MIN..=0) {
        let result = CustomerId::new(id);
        assert!(result.is_err());
    }
}
```

### Inline Test Modules

Rust uses `#[cfg(test)] mod tests` for inline unit tests —
they live inside the source file and have access to private
items, which is useful for testing internal invariants.

## Code Style and Tooling

### Required Tools

- `cargo fmt` before every commit (consistent formatting)
- `cargo clippy` with zero warnings — clippy catches
  correctness and performance issues the compiler misses
- `cargo test` must pass

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

### Clean Builds

Use `cargo clean` to remove cached build artifacts before
quality checks — stale incremental compilation state can
hide errors:

- `cargo fmt` — format
- `cargo clippy` — lint
- `cargo test` — test
- `cargo clean` — clean artifacts

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
