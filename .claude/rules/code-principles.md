---
paths:
  - "**/*.ts"
  - "**/*.tsx"
  - "**/*.py"
  - "**/*.rs"
  - "**/*.go"
---

# Code Design Principles

These principles apply when writing or modifying source
code. They complement the universal simplicity principles
(KISS, YAGNI, Reveals Intent, Fewest Elements) with
code-specific guidance.

## Kent Beck's Four Rules of Simple Design

Apply in priority order — never violate a higher-priority
rule for a lower one. These rules produce emergent design
through disciplined application:

1. **Tests Pass** — all tests must pass. Highest priority.
   Never compromise working code for cleaner structure.
2. **Reveals Intent** — code should clearly express what
   it does. Use meaningful names, clear function
   signatures, and domain types.
3. **No Duplication (DRY)** — extract common patterns,
   but not at the cost of clarity. Accidental similarity
   is not duplication.
4. **Fewest Elements** — minimize types, functions, and
   abstractions. Only add complexity that serves a clear
   purpose.

### When Rule 3 Conflicts with Rule 2

Choose clarity. Duplication is acceptable when:

- The duplicated code serves different purposes
  (accidental similarity)
- Removing duplication would create unclear abstractions
- The coupling from sharing code is worse than duplication
- Domain concepts are similar but will likely diverge

## SOLID Principles

### Single Responsibility (SRP)

A module should have one, and only one, reason to change.

### Open-Closed (OCP)

Software entities should be open for extension but closed
for modification — extending behavior through new code
(new types, new implementations) is safer than modifying
existing code that other modules depend on.

### Liskov Substitution (LSP)

Subtypes must be substitutable for their base types without
altering program correctness — violating this breaks
polymorphism and forces callers to know about specific
implementations.

### Interface Segregation (ISP)

No client should be forced to depend on methods it does
not use — large interfaces create unnecessary coupling and
make testing harder because mocks must implement irrelevant
methods.

### Dependency Inversion (DIP)

Depend on abstractions, not concretions — this makes
high-level business logic independent of infrastructure
details (databases, HTTP clients, file systems) and
enables testing with test doubles.

## Type-Driven Design

Use the type system to make invalid states
unrepresentable — this shifts error detection from runtime
to compile time, eliminating entire categories of bugs:

- Wrap primitives in domain types (newtypes/branded types)
  to prevent mix-ups
- Use discriminated unions/enums for state machines so the
  compiler enforces valid transitions
- Prefer `Result`/`Option` types over null/undefined to
  make failure handling explicit
- Clear function signatures serve as machine-checked
  documentation

## Testing Principles

### Test Independence

Each test must run in isolation — shared mutable state
between tests creates order-dependent failures that only
surface in CI and are expensive to debug. Tests should set
up their own preconditions and clean up after themselves.

### Test Naming

Test names should read as behavior specifications — a
failing test name should tell you what broke without reading
the test body:

- Good: `rejects_negative_value`, `should reject empty list`
- Bad: `test1`, `testError`, `test_calculate`

See the language-specific rule files for naming syntax
conventions per language.

### Deterministic Randomness

Seed random number generators explicitly in tests — unseeded
PRNGs produce different sequences on every run, making
failures non-reproducible and converting flakey tests into
noise that cannot be debugged. Use a fixed seed (a constant
or one derived from the test name) so any failure can be
reproduced exactly by re-running with the same seed. When
using property-based or fuzz testing libraries, verify that
the library persists failing seeds automatically (most do)
so a CI failure is always replayable locally.

## Red Flags

- Code that's hard to name (unclear responsibility)
- Copy-paste programming (unextracted duplication)
- Premature abstractions (violates Fewest Elements)
- Deep inheritance or trait/interface hierarchies
- Overly complex generic type constraints
- Using raw primitives instead of domain-specific types
- Panic/crash in production code paths
- Unnecessary data copying or allocation
