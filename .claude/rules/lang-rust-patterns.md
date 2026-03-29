---
paths:
  - "**/*.rs"
---

# Rust Patterns

Rust-specific implementations of functional, domain-driven,
and async patterns. For cross-language principles, see
`functional-style.md`.

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
