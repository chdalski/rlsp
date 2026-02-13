# Go Language Extension

> Extends base principles from `knowledge/base/principles.md`

## Philosophy

Go values simplicity, readability, and pragmatism. The
language deliberately omits features like generics-heavy
abstractions, inheritance, and exceptions in favor of
explicit error handling, composition, and straightforward
code. Write Go that reads like well-structured prose:
clear, direct, and unsurprising.

## Go Idioms

### Accept Interfaces, Return Structs

Define narrow interfaces at the consumer side:

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

Use struct embedding for code reuse:

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

Design types so their zero value is usable:

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

Keep interfaces small (1-3 methods):

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

// Wrap with context using fmt.Errorf
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

Handle errors immediately; do not defer checks:

```go
// Good - handle each error inline
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

- `cmd/` - application entry points
- `internal/` - private application code
- `pkg/` - library code usable by external projects

## Workflow Details

### Test File Ownership

Go places tests in `_test.go` files alongside source files
in the same package. When the Test Engineer and Developer
share a package, the Test Engineer creates the `_test.go`
file first. The Developer implements the production code in
the corresponding source file.

### Clean Builds

Use `go clean -cache -testcache` to remove cached build and
test results before quality checks. This ensures test
results reflect the current source, not cached passes.

### Build Tool Commands

- `gofmt -w .` — format code
- `go vet ./...` — correctness checks
- `golangci-lint run` — comprehensive linting
- `go test ./...` — run all tests
- `go clean -cache -testcache` — remove cached results

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

### Nil Interface Gotcha

```go
// This is non-nil even though the value is nil
var p *MyStruct = nil
var i interface{} = p
if i != nil {
    // This executes! i is non-nil interface
    // holding a nil pointer
}

// Fix - check the concrete value
func isNil(v interface{}) bool {
    return v == nil ||
        reflect.ValueOf(v).IsNil()
}
```

### Slice Append

```go
// Bug - append may modify original
func addItem(items []Item, item Item) []Item {
    return append(items, item)
}

// Fix - copy first if you need independence
func addItem(items []Item, item Item) []Item {
    result := make([]Item, len(items), len(items)+1)
    copy(result, items)
    return append(result, item)
}
```

### Loop Variable Capture

```go
// Bug (Go < 1.22) - all goroutines see last value
for _, id := range ids {
    go func() {
        process(id) // captures loop variable
    }()
}

// Fix - pass as argument (or use Go 1.22+)
for _, id := range ids {
    go func(id OrderID) {
        process(id)
    }(id)
}
```
