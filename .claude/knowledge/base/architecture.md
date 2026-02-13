# Architecture Patterns

## Hexagonal Architecture (Ports & Adapters)

Separate business logic from infrastructure by organizing
code into three layers with strict dependency rules.

### Layers

**Domain (center)**

- Contains business logic, entities, value objects, and
  domain services
- Has zero external dependencies — only the language's
  standard library
- Defines ports (interfaces/traits) that describe what it
  needs from the outside world
- Never imports from inbound or outbound layers

**Inbound adapters (driving side)**

- Convert external input into domain calls
- Examples: HTTP handlers, CLI parsers, message consumers,
  gRPC services
- Depend on the domain layer — never the reverse
- Validate and transform input into domain types before
  calling domain logic
- Map domain results back to the adapter's output format

**Outbound adapters (driven side)**

- Implement the ports defined by the domain
- Examples: database repositories, API clients, file
  storage, message publishers
- Depend on the domain layer — never the reverse
- Handle serialization, connection management, and
  infrastructure concerns

### Dependency Rule

Dependencies point inward. Infrastructure depends on the
domain. The domain depends on nothing external.

```text
Inbound Adapters → Domain ← Outbound Adapters
```

The domain defines ports (interfaces/traits). Adapters
implement them. The domain never knows which adapter is
behind the port — it works with the abstraction.

### Ports

Ports are the contracts between the domain and the outside
world. They are defined as interfaces or traits in the
domain layer.

- **Inbound ports** define what the domain offers (service
  interfaces). Inbound adapters call these.
- **Outbound ports** define what the domain needs
  (repository interfaces, external service interfaces).
  Outbound adapters implement these.

Keep ports focused and cohesive. A port should represent
one capability, not a grab-bag of methods.

### Value Objects and Newtypes

Wrap primitive types in domain-specific types that
validate their invariants at construction time.

- Constructor validates and returns a result or error —
  invalid states are not representable
- Once constructed, the value is guaranteed valid
- Provides compile-time safety against mixing up
  parameters of the same primitive type
- Co-locate unit tests with the value type definition

```pseudocode
type OrderId:
    value: integer

    new(raw: integer) -> Result<OrderId, Error>:
        if raw < 0:
            return Error("must be non-negative")
        return OrderId(raw)

    value() -> integer:
        return self.value
```

### Composition Root

Wire everything together at the application entry point,
not inside the layers.

- Create outbound adapters (repositories, clients)
- Create domain services, injecting outbound adapters
  via their port types
- Create inbound adapters (handlers), injecting domain
  services
- Start the application (server, lambda, CLI)

This is the only place that knows all the concrete types.
Every other module works with abstractions.

The layered structure enables targeted testing — see
`testing.md`.

## When to Use Hexagonal Architecture

Use it when:

- The domain has meaningful business logic worth isolating
- The application has multiple adapters (HTTP + CLI, or
  multiple data sources)
- Testability of domain logic without infrastructure is
  valuable
- The project will evolve over time and adapters may change

Do not use it when:

- The application is a thin pass-through (e.g., a proxy,
  a simple CRUD wrapper with no business rules)
- The overhead of ports and adapters exceeds the
  complexity of the domain
- The project is a script or one-off tool
