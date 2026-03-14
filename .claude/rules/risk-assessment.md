# Risk and Uncertainty Assessment

When deciding whether to consult advisors before
implementing a task, assess its risk and uncertainty using
the indicators below. This framework ensures advisors are
consulted when their expertise adds value, and skipped when
the task is straightforward — mandatory consultation on
every task produces rubber-stamp sign-offs that dilute the
signal when real issues arise.

## When to Consult the Test Engineer

Consult the test-engineer for a test list when the task
has **high uncertainty** — you are unsure what to test or
the testing strategy is non-obvious:

- Design trade-offs to evaluate — multiple valid
  approaches make it unclear which behaviors to assert
- Complex interactions between components — integration
  points where failures are subtle and hard to predict
- Greenfield code with no existing test patterns — no
  existing tests to follow as examples
- The task adds or modifies public API surface — API
  contracts need explicit coverage because callers depend
  on them

## When to Consult the Security Engineer

Consult the security-engineer for a security assessment
when the task has **high risk** — the blast radius of
getting it wrong includes security implications:

- **Trust boundaries** — code that sits between trusted and
  untrusted contexts (e.g., parsing user-supplied input,
  handling authentication/authorization)
- **Untrusted input** — deserialization, schema validation,
  file path handling, URL parsing from external sources
- **Cryptographic operations** — key management, token
  generation, signature verification, hashing
- **Network-facing code** — HTTP handlers, WebSocket
  endpoints, API routes exposed to clients
- **Secrets handling** — configuration that touches
  credentials, tokens, API keys, connection strings
- **Permission/access control** — code that decides what
  users can see or do
- **Data persistence** — SQL queries, file writes, cache
  operations where injection or corruption is possible

## When to Skip Advisors

Skip both advisors when the task is **low risk and low
uncertainty** — the implementation is straightforward and
the blast radius is contained:

- **Pure functions** — no I/O, no side effects, no external
  input
- **Internal wiring** — module registration, capability
  flags, handler delegation to existing functions
- **Pattern-following** — code structurally identical to
  existing, reviewed code in the same codebase
- **Test-only changes** — adding or modifying tests without
  changing production code
- **Refactoring** — restructuring code without changing
  behavior or trust boundaries
- **Documentation** — comments, README updates, plan files

## When in Doubt, Consult

The cost of an unnecessary consultation (a few seconds of
advisor time) is far lower than the cost of missing a
security gap or writing inadequate tests.
