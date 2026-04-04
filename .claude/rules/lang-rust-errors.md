---
paths:
  - "**/*.rs"
---

# Rust Error Handling

Rust's error handling leverages the type system to make
failure paths explicit and composable. Use `Result` and
`Option` consistently — `unwrap()` and `expect()` in
production code are panics waiting to happen.

## Crate Choices

- `thiserror` for library/domain error type definitions
- `anyhow` for application-level error handling
- Never use `unwrap()` or `expect()` in production code —
  they panic and crash the process

## Custom Error Types

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

## Error Propagation

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
