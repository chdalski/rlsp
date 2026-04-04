---
paths:
  - "**/*.go"
---

# Go

Go values simplicity, readability, and pragmatism. Write
Go that reads like well-structured prose: clear, direct,
and unsurprising.

## Go Idioms

### Accept Interfaces, Return Structs

Define narrow interfaces at the consumer side — this keeps
coupling low and makes testing easy:

```go
// Good - small interface defined by consumer
type OrderFinder interface {
    FindOrder(ctx context.Context, id OrderID) (*Order, error)
}

// Concrete implementation returns struct
func NewPostgresRepo(db *sql.DB) *PostgresRepo {
    return &PostgresRepo{db: db}
}
```

### Composition Over Inheritance

Use struct embedding for code reuse — Go has no
inheritance, and embedding gives you delegation without
the fragile base class problem:

```go
type BaseRepository struct{ db *sql.DB }

func (r *BaseRepository) execContext(ctx context.Context, query string, args ...any) (sql.Result, error) {
    return r.db.ExecContext(ctx, query, args...)
}

type OrderRepository struct{ BaseRepository }
```

### Zero Values Are Useful

Design types so their zero value is usable — this removes
the need for constructors in many cases:

```go
// sync.Mutex zero value is an unlocked mutex
var mu sync.Mutex

// bytes.Buffer zero value is an empty buffer
var buf bytes.Buffer
buf.WriteString("hello")
```

## Type System

### Structs for Domain Models

```go
type OrderID int64

type Order struct {
    ID        OrderID
    Items     []OrderItem
    Status    OrderStatus
    CreatedAt time.Time
}

type OrderItem struct {
    ProductID ProductID
    Quantity  int
    Price     Money
}
```

### Interfaces for Behavior

Keep interfaces small (1-3 methods) — large interfaces
are hard to implement and mock:

```go
type Reader interface {
    Read(p []byte) (n int, err error)
}

type OrderRepository interface {
    Find(ctx context.Context, id OrderID) (*Order, error)
    Save(ctx context.Context, order *Order) error
}
```

### Type Assertions and Switches

```go
func handleError(err error) {
    switch e := err.(type) {
    case *NotFoundError:
        log.Printf("not found: %s", e.Entity)
    case *ValidationError:
        log.Printf("validation: %s", e.Field)
    default:
        log.Printf("unexpected: %v", err)
    }
}
```

## Code Style and Tooling

### Required Tools

- `gofmt` for formatting (automatic, non-negotiable)
- `go vet` for correctness checks
- `golangci-lint` for comprehensive linting
- `go clean -cache -testcache` before quality checks — cached passes can hide regressions

### Style Guidelines

- Follow Effective Go and the Go Code Review Comments
- Keep functions short and focused
- Use meaningful names (no single-letter except in loops)
- Package names: short, lowercase, no underscores
- Exported names are the package's API; be deliberate

### Naming Conventions

```go
// Package names - short, lowercase
package orders

// Interfaces - typically end with -er for single method
type Reader interface { ... }
type Validator interface { ... }

// Getters - no "Get" prefix
func (o *Order) Status() OrderStatus { return o.status }

// Acronyms - all caps
type HTTPClient struct { ... }
type OrderID int64
```

## Project Structure

Follow the standard Go project layout — `cmd/` for entry
points, `internal/` for private code, `pkg/` for reusable
libraries:

```text
project/
  cmd/server/main.go
  internal/
    orders/
      order.go
      order_test.go
      repository.go
    users/user.go
  pkg/money/money.go
  go.mod
```

## Common Pitfalls

| Pitfall | Why It's Bad | Fix |
|---|---|---|
| Goroutine leaks | Memory/resource leaks | Use context cancellation |
| Nil interface | Non-nil interface with nil value | Check concrete value |
| Slice append gotcha | May modify underlying array | Copy when sharing |
| Closing over loop var | All goroutines share same var | Capture in func arg |
| Ignoring errors | Silent failures | Always handle errors |
| Bare `panic()` | Crashes the program | Return errors instead |
| Large interfaces | Hard to mock and test | Keep interfaces small |
| Package-level state | Hard to test | Dependency injection |
