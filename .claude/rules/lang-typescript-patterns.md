---
paths:
  - "**/*.ts"
  - "**/*.tsx"
---

# TypeScript — Patterns

TypeScript-specific patterns for functional programming,
React components, and Node.js. For cross-language functional
principles, see `functional-style.md`.

## Functional Patterns

### Immutability

Prefer immutable operations — mutation creates hidden
coupling between call sites that share references:

- Use `readonly` for properties and arrays
- Use `as const` for literal types
- Prefer spread operators over mutation
- Use `Object.freeze` for runtime enforcement

```typescript
// Bad - mutation
function addItem(order: Order, item: Item): void {
  order.items.push(item);
}

// Good - return new value
function addItem(order: Order, item: Item): Order {
  return {
    ...order,
    items: [...order.items, item],
  };
}
```

### Array Transformations

Use `map`, `filter`, `find` instead of imperative loops
when the criteria in `functional-style.md` are met
(readability, less code, no manual index math, lower
complexity).

**Collect-and-push** — the most common anti-pattern.
Always refactor:

```typescript
// Anti-pattern — mutable accumulator + loop
const results: string[] = [];
for (const user of users) {
  if (user.isActive) {
    results.push(user.name.toUpperCase());
  }
}

// Refactored — declarative, no mutation
const results = users
  .filter((user) => user.isActive)
  .map((user) => user.name.toUpperCase());
```

**`.reduce()` readability caveat** — `reduce` with a
complex accumulator (building objects with multiple keys,
tracking state across iterations) is often less readable
than a loop. When the reducer callback exceeds 3-4 lines
or needs type assertions on the accumulator, use a loop
instead:

```typescript
// reduce with complex accumulator — hard to follow
const grouped = items.reduce<Record<string, Item[]>>(
  (acc, item) => {
    const key = item.category;
    acc[key] = [...(acc[key] ?? []), item];
    return acc;
  },
  {}
);

// Loop is clearer for complex accumulation
const grouped: Record<string, Item[]> = {};
for (const item of items) {
  const key = item.category;
  (grouped[key] ??= []).push(item);
}
```

**When loops are correct in TypeScript:**

- **`for await...of` with multiple awaits** — a loop with
  `await` per iteration and `try`/`catch` is clearer than
  piping through async transform streams. Async stream
  combinators add ceremony without improving readability.
- **Complex `reduce` accumulators** — when the callback
  needs multiple lines, type assertions, or tracks more
  than one piece of state, a loop is more readable (see
  example above).
- **Index-dependent mutation** — when each iteration
  depends on results from previous iterations in ways that
  require accessing the partially-built result.

See `functional-style.md` for the full decision criteria.

### Pipeline Pattern

```typescript
const processOrder = (input: RawOrder): Result<Order, Error> =>
  pipe(
    input,
    validate,
    enrich,
    calculateTotals,
    applyDiscounts
  );

// Without a pipe utility, use intermediate variables
function processOrder(input: RawOrder): Result<Order, Error> {
  const validated = validate(input);
  if (!validated.ok) return validated;
  const enriched = enrich(validated.value);
  const totaled = calculateTotals(enriched);
  return applyDiscounts(totaled);
}
```

## React Patterns

### Functional Components

Always use functional components with hooks — class
components are legacy and don't compose as well:

```typescript
interface UserProfileProps {
  readonly userId: string;
  readonly onUpdate: (user: User) => void;
}

function UserProfile({
  userId,
  onUpdate,
}: UserProfileProps): JSX.Element {
  const [user, setUser] = useState<User | null>(null);

  useEffect(() => {
    fetchUser(userId).then(setUser);
  }, [userId]);

  if (!user) return <Loading />;
  return <ProfileView user={user} onUpdate={onUpdate} />;
}
```

### Composition Over Prop Drilling

```typescript
function OrderPage(): JSX.Element {
  return (
    <OrderProvider>
      <OrderHeader />
      <OrderItems />
      <OrderSummary />
    </OrderProvider>
  );
}
```

### Custom Hooks for Logic Extraction

```typescript
function useOrder(orderId: string) {
  const [order, setOrder] = useState<Order | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchOrder(orderId)
      .then(setOrder)
      .catch(setError)
      .finally(() => setLoading(false));
  }, [orderId]);

  return { order, error, loading } as const;
}
```

## Node.js Patterns

### Async/Await

Always use async/await over raw promises or callbacks —
callback nesting obscures control flow and error handling:

```typescript
// Good - async/await
async function getUserOrders(id: string): Promise<Order[]> {
  const user = await fetchUser(id);
  return fetchOrders(user.id);
}
```

### Error Handling

Define specific error classes and handle errors at
boundaries — generic catches hide the root cause:

```typescript
class NotFoundError extends Error {
  constructor(
    readonly entity: string,
    readonly id: string
  ) {
    super(`${entity} not found: ${id}`);
    this.name = "NotFoundError";
  }
}

async function handleRequest(
  req: Request,
  res: Response
): Promise<void> {
  try {
    const result = await processRequest(req);
    res.json(result);
  } catch (error) {
    if (error instanceof NotFoundError) {
      res.status(404).json({ error: error.message });
    } else {
      res.status(500).json({ error: "Internal error" });
    }
  }
}
```
