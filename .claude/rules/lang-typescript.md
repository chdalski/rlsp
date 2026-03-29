---
paths:
  - "**/*.ts"
  - "**/*.tsx"
---

# TypeScript

TypeScript adds static types to JavaScript, catching errors
at compile time. Embrace strict mode, discriminated unions,
and type narrowing to make invalid states unrepresentable.
Prefer functional patterns and immutability while staying
pragmatic about JavaScript's multi-paradigm nature.

## Type System

### Strict Mode

Always enable strict mode in `tsconfig.json` — without it,
TypeScript silently permits `null` and `undefined` in places
that cause runtime crashes:

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

Make invalid states unrepresentable with tagged unions —
the compiler enforces exhaustive handling, so adding a new
variant surfaces every location that needs updating:

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

Use type guards and narrowing instead of type assertions —
assertions bypass the compiler's safety checks and hide
bugs:

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
  console.log(response.items.length);
}
```

### Branded Types (Newtypes)

Avoid primitive obsession with branded types — the compiler
prevents passing a `CustomerId` where an `OrderId` is
expected, catching mix-ups that raw `number` allows:

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

Use built-in utility types — they express intent clearly
and reduce boilerplate:

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

## Code Style

### Tools

- **ESLint** with typescript-eslint for linting
- **Prettier** for formatting
- Enable `no-explicit-any` and `no-unused-vars` rules
- Remove `node_modules/.cache`, `dist/`, or framework-specific build directories before quality checks — stale build artifacts can mask errors

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

### Quality Commands

- `npm run format` / `npx prettier --write .` — format
- `npm run lint` / `npx eslint .` — lint
- `npm test` / `npx vitest` / `npx jest` — test
- `rm -rf dist node_modules/.cache` — clean artifacts

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
| Accumulate-in-loop | Higher code mass, mutable state | Use `filter`/`map`/`find` |
| Complex `reduce` | Unreadable accumulator | Use a loop instead |
