# Rust Language Extension

> Extends base principles from `knowledge/base/principles.md`

## Philosophy

Rust prioritizes safety, performance, and expressiveness.
The compiler enforces memory safety and thread safety at
compile time. Embrace the type system and borrow checker
as design tools, not obstacles. Rust's ownership model
naturally aligns with functional programming and
immutability-first design.

## Ownership, Borrowing, and Lifetimes

### Ownership Design

- Prefer owned types for public API boundaries
- Use references (`&T`, `&mut T`) to avoid unnecessary clones
- Keep lifetimes explicit only when the compiler requires it
- Use `Arc` or `Rc` for shared ownership scenarios
- Use `Cow<'_, T>` for data that may or may not be owned

### Red Flags

- **Excessive `.clone()`** indicates poor ownership design;
  restructure data flow instead
- **Fighting the borrow checker** means the design needs
  rethinking, not workarounds
- **Using `String` when `&str` suffices** creates unnecessary
  allocations

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

Avoid primitive obsession. Wrap raw types to encode domain
meaning and validation:

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

Use enums to make invalid states unrepresentable:

```rust
enum OrderStatus {
    Pending { created_at: DateTime<Utc> },
    Confirmed { confirmed_at: DateTime<Utc> },
    Shipped { tracking: TrackingNumber },
    Delivered { delivered_at: DateTime<Utc> },
}
```

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
- Never use `unwrap()` or `expect()` in production code

### Custom Error Types

Define specific error types per module or domain:

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

## Functional Patterns in Rust

### Iterator Chains Over Loops

Prefer declarative iterator chains over imperative loops.
This aligns with lower code mass (see base principles).

```rust
// Imperative (avoid)
let mut results = Vec::new();
for item in items {
    if item.is_valid() {
        results.push(item.transform());
    }
}

// Declarative (preferred)
let results: Vec<_> = items
    .iter()
    .filter(|item| item.is_valid())
    .map(|item| item.transform())
    .collect();
```

### Code Mass Analysis for Iterators

Transforming methods count as Loops (mass 5):
`.map()`, `.filter()`, `.flat_map()`, `.fold()`,
`.take()`, `.skip()`, `.zip()`, `.chain()`

Consuming methods count as Invocations (mass 2):
`.collect()`, `.sum()`, `.count()`, `.any()`,
`.all()`, `.find()`, `.min()`, `.max()`

### Immutability by Default

- `let` bindings are immutable; use `mut` only when needed
- Expression-based language reduces need for assignments
- Prefer transformations over in-place mutation

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

Build complex operations from small, composable functions:

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

## Domain-Driven Design in Rust

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

### Aggregates as Structs with Invariant Enforcement

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
- Avoid blocking operations in async code
- Use `tokio::spawn` for concurrent tasks
- Use channels for inter-task communication
- Document whether functions are CPU-bound or I/O-bound

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
- Tests go in `#[cfg(test)] mod tests` within each file

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

### TDD with Ignored Test Lists

Start features with a full list of ignored tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn should_accept_valid_email() {}

    #[test]
    #[ignore]
    fn should_reject_email_without_at_sign() {}

    #[test]
    #[ignore]
    fn should_reject_empty_email() {}
}
```

Remove `#[ignore]` one at a time and implement
minimally to pass each test.

### Property-Based Testing

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

## Code Style and Tooling

### Required Tools

- `cargo fmt` before every commit (consistent formatting)
- `cargo clippy` with zero warnings
- `cargo test` must pass

### Style Guidelines

- Functions under 50 lines; prefer early returns
- Limit nesting depth
- Meaningful names; avoid abbreviations
- Comments explain "why", not "what"

### Module Organization

- Use `<module>.rs` files, NOT `mod.rs` in `src/`
  - For submodules: `domain.rs` with `domain/models.rs`
  - Exception: `mod.rs` is acceptable in `tests/`
- Organize by feature/domain, not by technical layer
- Re-export public APIs with `pub use`
- Use `snake_case` for file and folder names

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

## Workflow Details

### Inline Test Modules

Rust uses `#[cfg(test)] mod tests` for inline unit tests.
When the Test Engineer and Developer share a file, the Test
Engineer creates the file with the `#[cfg(test)]` module
first. The Developer implements the production code above it.

### Clean Builds

Use `cargo clean` to remove cached build artifacts before
quality checks. This ensures clippy and test results reflect
the current source, not stale incremental compilation state.

### Build Tool Commands

- `cargo fmt` — format code
- `cargo clippy` — lint (run with zero warnings)
- `cargo test` — run all tests
- `cargo clean` — remove build artifacts

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
| Security | `secrecy` | Sensitive data |
| Collections | `im` | Immutable collections |
