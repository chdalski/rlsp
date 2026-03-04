---
paths:
  - "**/*.ts"
  - "**/*.tsx"
  - "**/*.py"
  - "**/*.rs"
---

# Functional Style

These guidelines define functional programming principles
for languages that support them well (TypeScript, Python,
Rust). Go is excluded — Go's pragmatic style favors explicit
loops and mutation over FP abstractions.

## Core Principles

### Functions as First-Class Citizens

Use higher-order functions (`map`, `filter`, `reduce`) over
explicit loops — they declare intent and eliminate off-by-one
errors and mutable accumulators:

- Pass functions as arguments to enable abstraction
- Return functions from other functions
- Store functions in data structures when appropriate
- Use dynamic dispatch for runtime polymorphism when needed

### Deterministic Functions

Same inputs must always produce the same output — this
makes functions testable in isolation and safe to parallelize:

- Avoid dependencies on mutable global state
- Avoid time-dependent or random operations in pure
  functions
- Make external dependencies explicit through parameters
- Use pure functions for business logic and calculations

### Avoid Side Effects

Separate pure computation from I/O — this keeps business
logic testable without mocking and makes the I/O boundary
explicit:

- Mark functions with side effects clearly (naming, types)
- Keep side effects at the boundaries (adapters, I/O layer)
- Return values instead of mutating state
- **Return decisions, not actions** — encode what should
  happen as a data structure (an enum of possible actions,
  an intermediate result type) rather than performing the
  action inside the function; callers handle the effect,
  tests verify the decision without triggering any I/O

### Immutable Data

Prefer immutable bindings by default — mutation creates
hidden coupling between call sites that share references:

- Use owned values and transformations over in-place
  mutation
- Copy when necessary rather than sharing mutable
  references
- Consider persistent/immutable data structures for
  complex state

### Declarative Style

Express what to compute, not how — declarative code reads
as a specification of the result rather than a recipe of
steps:

- Use collection pipelines over manual loops
- Leverage pattern matching for control flow
- Use method chaining for data transformations
- Prefer expressions over statements

### Function Composition

Build complex operations from small, composable functions —
each step is independently testable and reusable:

- Create small, focused functions that do one thing well
- Combine functions using pipelines and combinators

## Practical Guidelines

- **Balance pragmatism with principles**: Use mutation when
  it is clearer or more performant — strict FP can create
  abstractions that hurt readability or performance, and
  clarity always wins (see `simplicity.md`)
- **Use Result/Option types**: These are functional patterns
  for error handling and absence
- **Leverage type-driven design**: Make invalid states
  unrepresentable
- **Prefer map/flatMap/orElse** over explicit branching
  when appropriate
- **Extract pure logic**: Isolate business rules in pure
  functions, keeping effects at boundaries
- **Document impure functions**: Make side effects visible
  in function signatures or documentation
- **Recursion vs iteration**: Use recursion for naturally
  recursive problems (trees, graphs); prefer iteration
  for flat collections
