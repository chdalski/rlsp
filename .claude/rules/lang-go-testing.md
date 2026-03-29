---
paths:
  - "**/*.go"
---

# Go Testing

Go's testing package and conventions emphasize simplicity
and co-location. Tests live in `_test.go` files alongside
source files, keeping them close to the code they verify
and letting them access unexported identifiers when needed.

## Table-Driven Tests

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

## Testify Assertions

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

## Test Helpers

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

## Mocking with Interfaces

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

## Test File Conventions

Go places tests in `_test.go` files alongside source files
in the same package. This co-location keeps tests close to
the code they verify and lets tests access unexported
identifiers when needed.

## Design for Testability

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
pitfall in `lang-go.md` — injectable dependencies over
hardcoded effects.
