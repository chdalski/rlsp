# TypeScript Language Extension

> Extends base principles from `knowledge/base/principles.md`

## Philosophy

TypeScript adds static types to JavaScript, catching errors
at compile time. Embrace strict mode, discriminated unions,
and type narrowing to make invalid states unrepresentable.
Prefer functional patterns and immutability while staying
pragmatic about JavaScript's multi-paradigm nature.

## Type System

### Strict Mode

Always enable strict mode in `tsconfig.json`:

```json
{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true
  }
}
```

### Discriminated Unions

Make invalid states unrepresentable with tagged unions:

```typescript
type Result<T, E> =
  | { ok: true; value: T }
  | { ok: false; error: E };

type OrderStatus =
  | { type: "pending"; createdAt: Date }
  | { type: "confirmed"; confirmedAt: Date }
  | { type: "shipped"; tracking: string }
  | { type: "delivered"; deliveredAt: Date };
```

### Type Narrowing

Use type guards and narrowing instead of type assertions:

```typescript
// Bad - type assertion
const order = response as Order;

// Good - type guard
function isOrder(value: unknown): value is Order {
  return (
    typeof value === "object" &&
    value !== null &&
    "id" in value &&
    "items" in value
  );
}

if (isOrder(response)) {
  // TypeScript knows response is Order here
  console.log(response.items.length);
}
```

### Branded Types (Newtypes)

Avoid primitive obsession with branded types:

```typescript
type CustomerId = number & { readonly __brand: "CustomerId" };
type Email = string & { readonly __brand: "Email" };

function CustomerId(value: number): CustomerId {
  if (value <= 0) throw new Error("ID must be positive");
  return value as CustomerId;
}

function Email(value: string): Email {
  if (!value.includes("@")) throw new Error("Invalid email");
  return value as Email;
}
```

### Utility Types

Use built-in utility types effectively:

```typescript
// Immutable objects
type Config = Readonly<{
  apiUrl: string;
  timeout: number;
}>;

// Partial updates
function updateOrder(
  order: Order,
  changes: Partial<Pick<Order, "name" | "address">>
): Order {
  return { ...order, ...changes };
}

// Const assertions for literal types
const STATUSES = ["pending", "active", "closed"] as const;
type Status = (typeof STATUSES)[number];
```

## Functional Patterns

### Immutability

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

Use `map`, `filter`, `reduce` over imperative loops:

```typescript
// Imperative (avoid)
const results: string[] = [];
for (const user of users) {
  if (user.isActive) {
    results.push(user.name.toUpperCase());
  }
}

// Declarative (preferred)
const results = users
  .filter((user) => user.isActive)
  .map((user) => user.name.toUpperCase());
```

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

Always use functional components with hooks:

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
// Use composition and context
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

Always use async/await over raw promises or callbacks:

```typescript
// Bad - callback hell
fetchUser(id, (err, user) => {
  if (err) return handleError(err);
  fetchOrders(user.id, (err, orders) => {
    if (err) return handleError(err);
    // ...
  });
});

// Good - async/await
async function getUserOrders(id: string): Promise<Order[]> {
  const user = await fetchUser(id);
  return fetchOrders(user.id);
}
```

### Error Handling

```typescript
// Define specific error classes
class NotFoundError extends Error {
  constructor(
    readonly entity: string,
    readonly id: string
  ) {
    super(`${entity} not found: ${id}`);
    this.name = "NotFoundError";
  }
}

// Handle errors at boundaries
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

## Testing

### Frameworks

- **Jest** or **Vitest** for unit and integration tests
- **React Testing Library** for component tests
- **MSW** (Mock Service Worker) for API mocking

### Test Structure

```typescript
describe("OrderService", () => {
  describe("createOrder", () => {
    it("should create order with valid items", async () => {
      const items = [createItem({ quantity: 2 })];

      const result = await service.createOrder(items);

      expect(result.ok).toBe(true);
      expect(result.value.items).toHaveLength(1);
    });

    it("should reject empty item list", async () => {
      const result = await service.createOrder([]);

      expect(result.ok).toBe(false);
      expect(result.error.code).toBe("EMPTY_ORDER");
    });
  });
});
```

### Component Testing

```typescript
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

test("submits order on button click", async () => {
  const onSubmit = vi.fn();
  render(<OrderForm onSubmit={onSubmit} />);

  await userEvent.click(
    screen.getByRole("button", { name: /submit/i })
  );

  expect(onSubmit).toHaveBeenCalledOnce();
});
```

### MSW for API Mocking

```typescript
import { http, HttpResponse } from "msw";
import { setupServer } from "msw/node";

const server = setupServer(
  http.get("/api/orders/:id", ({ params }) => {
    return HttpResponse.json({
      id: params.id,
      status: "pending",
    });
  })
);

beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());
```

## Code Style

### Tools

- **ESLint** for linting (with typescript-eslint)
- **Prettier** for formatting
- Enable `no-explicit-any` and `no-unused-vars` rules

### Module Organization

- Feature-based directory structure
- Barrel exports (`index.ts`) for public APIs
- Co-locate tests with source files

```text
src/
  orders/
    order.ts
    order.test.ts
    order-service.ts
    order-service.test.ts
    index.ts
  users/
    user.ts
    user-service.ts
    index.ts
```

## Workflow Details

### Clean Builds

Remove `node_modules/.cache`, `dist/`, or framework-specific
build directories before quality checks. For projects using
a bundler, run the clean script if defined in `package.json`.

### Build Tool Commands

- `npm run format` / `npx prettier --write .` — format code
- `npm run lint` / `npx eslint .` — lint
- `npm test` / `npx vitest` / `npx jest` — run tests
- `rm -rf dist node_modules/.cache` — remove build artifacts

## Common Pitfalls

| Pitfall | Why It's Bad | Fix |
|---|---|---|
| `any` type | Bypasses type safety | Use `unknown` + narrowing |
| Missing null checks | Runtime crashes | Enable strict null checks |
| Callback hell | Unreadable code | Use async/await |
| Mutable shared state | Race conditions | Immutable patterns |
| Barrel re-exports everywhere | Circular deps | Use selectively |
| `as` type assertions | Unsafe casts | Type guards instead |
| Ignoring Promise rejections | Silent failures | Always handle errors |
| Enums | Surprising behavior | Use `as const` objects |
