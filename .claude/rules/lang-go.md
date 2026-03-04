---
paths:
  - "**/*.go"
---

# Go

Go values simplicity, readability, and pragmatism. The
language deliberately omits features like generics-heavy
abstractions, inheritance, and exceptions in favor of
explicit error handling, composition, and straightforward
code. Write Go that reads like well-structured prose:
clear, direct, and unsurprising.

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
type BaseRepository struct {
    db *sql.DB
}

func (r *BaseRepository) execContext(
    ctx context.Context,
    query string,
    args ...any,
) (sql.Result, error) {
    return r.db.ExecContext(ctx, query, args...)
}

type OrderRepository struct {
    BaseRepository
}
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

## Error Handling

### Error Wrapping

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

### Custom Error Types

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

### Error Checking

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

### Error Handling Pattern

Handle errors immediately — deferring checks makes control
flow harder to follow:

```go
user, err := repo.FindUser(ctx, id)
if err != nil {
    return fmt.Errorf("find user: %w", err)
}

orders, err := repo.FindOrders(ctx, user.ID)
if err != nil {
    return fmt.Errorf("find orders: %w", err)
}
```

## Concurrency

### Goroutines and Channels

```go
func processItems(
    ctx context.Context,
    items []Item,
) ([]Result, error) {
    results := make(chan Result, len(items))
    errs := make(chan error, 1)

    var wg sync.WaitGroup
    for _, item := range items {
        wg.Add(1)
        go func(item Item) {
            defer wg.Done()
            result, err := process(ctx, item)
            if err != nil {
                select {
                case errs <- err:
                default:
                }
                return
            }
            results <- result
        }(item)
    }

    go func() {
        wg.Wait()
        close(results)
        close(errs)
    }()

    select {
    case err := <-errs:
        return nil, err
    case <-ctx.Done():
        return nil, ctx.Err()
    default:
    }

    var collected []Result
    for r := range results {
        collected = append(collected, r)
    }
    return collected, nil
}
```

### errgroup for Structured Concurrency

Prefer errgroup over manual WaitGroup + error channel
patterns — it handles cancellation and error propagation:

```go
import "golang.org/x/sync/errgroup"

func fetchAll(
    ctx context.Context,
    ids []OrderID,
    repo OrderRepository,
) ([]*Order, error) {
    g, ctx := errgroup.WithContext(ctx)
    orders := make([]*Order, len(ids))

    for i, id := range ids {
        i, id := i, id
        g.Go(func() error {
            order, err := repo.Find(ctx, id)
            if err != nil {
                return err
            }
            orders[i] = order
            return nil
        })
    }

    if err := g.Wait(); err != nil {
        return nil, err
    }
    return orders, nil
}
```

### Sync Package

```go
// Use sync.Once for one-time initialization
var (
    instance *Service
    once     sync.Once
)

func GetService() *Service {
    once.Do(func() {
        instance = &Service{}
    })
    return instance
}

// Use sync.Map for concurrent map access
var cache sync.Map

func Get(key string) (Value, bool) {
    v, ok := cache.Load(key)
    if !ok {
        return Value{}, false
    }
    return v.(Value), true
}
```

## Testing

### Table-Driven Tests

Table-driven tests are idiomatic Go — they centralize test
data and make adding cases trivial:

```go
func TestCustomerID(t *testing.T) {
    tests := []struct {
        name    string
        input   int64
        wantErr bool
    }{
        {"positive value", 42, false},
        {"zero value", 0, true},
        {"negative value", -1, true},
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            _, err := NewCustomerID(tt.input)
            if (err != nil) != tt.wantErr {
                t.Errorf(
                    "NewCustomerID(%d) error = %v, "+
                        "wantErr %v",
                    tt.input, err, tt.wantErr,
                )
            }
        })
    }
}
```

### Testify Assertions

```go
import (
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestOrderCreation(t *testing.T) {
    order, err := NewOrder(validItems)

    require.NoError(t, err)
    assert.Equal(t, 3, len(order.Items))
    assert.Equal(t, StatusPending, order.Status)
}
```

### Test Helpers

```go
func newTestOrder(t *testing.T) *Order {
    t.Helper()
    order, err := NewOrder([]OrderItem{
        {ProductID: 1, Quantity: 2, Price: Money(1000)},
    })
    require.NoError(t, err)
    return order
}
```

### Mocking with Interfaces

Define test doubles that implement the interface — Go's
implicit interface satisfaction means no mocking framework
is required for simple cases:

```go
type mockRepo struct {
    orders map[OrderID]*Order
    err    error
}

func (m *mockRepo) Find(
    _ context.Context,
    id OrderID,
) (*Order, error) {
    if m.err != nil {
        return nil, m.err
    }
    order, ok := m.orders[id]
    if !ok {
        return nil, ErrNotFound
    }
    return order, nil
}
```

### Test File Conventions

Go places tests in `_test.go` files alongside source files
in the same package. This co-location keeps tests close to
the code they verify and lets tests access unexported
identifiers when needed.

### Design for Testability

Keep business logic free of direct I/O — functions that call
`fmt.Println` or write to files directly embed an effect
that tests cannot observe or redirect without capturing
stdout, making them slow and fragile.

Accept `io.Writer` for output instead — the standard library
uses this pattern everywhere, and tests can pass a
`bytes.Buffer` as a zero-cost in-memory sink:

```go
// Hard to test — output is embedded
func printReport(orders []Order) {
    for _, o := range orders {
        fmt.Printf("Order %d: %s\n", o.ID, o.Status)
    }
}

// Testable — caller controls where output goes
func writeReport(w io.Writer, orders []Order) error {
    for _, o := range orders {
        if _, err := fmt.Fprintf(
            w, "Order %d: %s\n", o.ID, o.Status,
        ); err != nil {
            return err
        }
    }
    return nil
}

// In tests
var buf bytes.Buffer
err := writeReport(&buf, orders)
require.NoError(t, err)
assert.Contains(t, buf.String(), "Order 1: pending")
```

For decision logic, return a struct describing what to do
and let the caller execute it — the test verifies the
decision without any I/O setup:

```go
type PricingDecision struct {
    ApplyDiscount bool
    DiscountPct   int
}

func evaluateOrder(order Order) PricingDecision {
    if order.Total > 100 {
        return PricingDecision{ApplyDiscount: true, DiscountPct: 10}
    }
    return PricingDecision{}
}
```

This aligns with Go's "Accept Interfaces, Return Structs"
idiom and is the same principle as the package-level state
pitfall in the table below — injectable dependencies over
hardcoded effects.

## Code Style and Tooling

### Required Tools

- `gofmt` for formatting (automatic, non-negotiable)
- `go vet` for correctness checks
- `golangci-lint` for comprehensive linting

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

Follow the standard Go project layout:

```text
project/
  cmd/
    server/
      main.go
  internal/
    orders/
      order.go
      order_test.go
      repository.go
      service.go
    users/
      user.go
  pkg/
    money/
      money.go
  go.mod
  go.sum
```

- `cmd/` — application entry points
- `internal/` — private application code
- `pkg/` — library code usable by external projects

### Clean Builds

Use `go clean -cache -testcache` to remove cached build and
test results before quality checks — cached passes can hide
regressions:

- `gofmt -w .` — format
- `go vet ./...` — correctness checks
- `golangci-lint run` — lint
- `go test ./...` — test
- `go clean -cache -testcache` — clean cache

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
