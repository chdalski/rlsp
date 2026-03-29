---
paths:
  - "**/*.go"
---

# Go Concurrency

Go's concurrency model uses goroutines and channels.
Prefer structured patterns over raw goroutine management
— unstructured concurrency leaks goroutines and makes
error propagation unreliable.

## Goroutines and Channels

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

## errgroup for Structured Concurrency

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

## Sync Package

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
