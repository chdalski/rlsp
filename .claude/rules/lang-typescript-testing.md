---
paths:
  - "**/*.ts"
  - "**/*.tsx"
---

# TypeScript — Testing

TypeScript-specific testing patterns and frameworks. For
cross-language testing principles (isolation, independence,
Arrange-Act-Assert), see `code-principles.md`.

## Frameworks

- **Jest** or **Vitest** for unit and integration tests
- **React Testing Library** for component tests
- **MSW** (Mock Service Worker) for API mocking

## Test Structure

Use `describe`/`it` blocks with Arrange-Act-Assert —
consistent structure makes tests scannable:

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

## Component Testing

Test through user-visible behavior, not implementation
details — tests that click buttons and check text survive
refactoring:

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

## MSW for API Mocking

Mock at the network boundary, not at the module level —
this tests the actual HTTP handling code:

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
