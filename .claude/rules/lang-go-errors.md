---
paths:
  - "**/*.go"
---

# Go Error Handling

Go's error handling is explicit — errors are values, not
exceptions. Every fallible function returns an error, and
callers must handle it. This verbosity is intentional: it
makes failure paths visible and prevents silent swallowing.

## Error Wrapping

Wrap errors with context using `fmt.Errorf` and `%w` —
unwrapped errors lose the chain of causation:

```go
import (
    "errors"
    "fmt"
)

// Define sentinel errors
var (
    ErrNotFound   = errors.New("not found")
    ErrValidation = errors.New("validation failed")
)

// Wrap with context
func (r *Repo) FindOrder(
    ctx context.Context,
    id OrderID,
) (*Order, error) {
    row := r.db.QueryRowContext(ctx, query, id)
    order, err := scanOrder(row)
    if errors.Is(err, sql.ErrNoRows) {
        return nil, fmt.Errorf(
            "order %d: %w", id, ErrNotFound,
        )
    }
    if err != nil {
        return nil, fmt.Errorf(
            "find order %d: %w", id, err,
        )
    }
    return order, nil
}
```

## Custom Error Types

```go
type NotFoundError struct {
    Entity string
    ID     int64
}

func (e *NotFoundError) Error() string {
    return fmt.Sprintf("%s not found: %d", e.Entity, e.ID)
}

func (e *NotFoundError) Is(target error) bool {
    return target == ErrNotFound
}
```

## Error Checking

```go
// Use errors.Is for sentinel errors
if errors.Is(err, ErrNotFound) {
    return http.StatusNotFound
}

// Use errors.As for typed errors
var validErr *ValidationError
if errors.As(err, &validErr) {
    return validErr.Field
}
```
