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

## Loops vs Declarative Alternatives

Use declarative alternatives (collection pipelines,
comprehensions, iterator chains) over imperative loops
**when all four criteria hold**, evaluated in priority
order — if any criterion fails, keep the loop:

1. **Readability** — the declarative version is at least
   as readable as the loop. This is the trump card. If the
   chain is harder to follow, stop here and keep the loop.
   Readability degrades when chains require complex
   combinators, deeply nested closures, or multi-field
   accumulator state.

2. **Less code** — the declarative version reduces code
   volume: fewer bindings, no mutable accumulator, no
   manual setup/teardown. If both versions are the same
   size, the refactor adds churn without benefit.

3. **No manual index math** — the loop uses index
   arithmetic (`i + offset`, `i - 1`, `len - i`) that the
   declarative alternative eliminates. Manual index math is
   a source of off-by-one errors that collection operations
   handle correctly by construction.

4. **Lower complexity** — the declarative version does not
   require complex combinators (fold/scan with tuple
   accumulators, nested flat-map with closures, async
   stream adapters with pinning or boxing). When the
   declarative version needs more machinery than the loop,
   the loop is simpler.

### When Loops Win

These patterns should remain as loops because the
declarative alternative fails one or more criteria above:

- **State machines** — loops tracking multiple mutable
  variables (depth counters, quote flags, parser state)
  through match arms. A scan/fold with a tuple accumulator
  is harder to read (fails criterion 1).

- **Async iteration with multiple await points** — when
  each iteration awaits one or more async operations with
  error propagation, a simple loop with `await` and `?`/
  `try`/`except` is clearer than async stream combinators
  that add ceremony (fails criteria 1 and 4).

- **Recursive tree/graph walks** — the recursive helper
  function is inherently imperative. The outer dispatch
  (`for node in nodes { walk(node, &mut acc) }`) could
  become `.flat_map()`, but the recursion itself cannot —
  forcing it into iterators requires a stack-based adapter
  that obscures the traversal logic (fails criterion 4).

- **Complex early-exit conditions** — loops with multiple
  interleaved break/continue conditions tied to different
  state. Expressing these as chained `.take_while()`,
  `.skip_while()`, `.filter()` produces a pipeline that is
  harder to reason about than the explicit control flow
  (fails criterion 1).

- **Test data builders** — loops that construct test
  fixtures. These are not production code; clarity matters
  more than style compliance.

### When Declarative Wins

These patterns should be refactored — the declarative
alternative satisfies all four criteria:

- **Collect-and-push** — `let mut v = []; for x { if cond
  { v.push(transform(x)) } }` → filter + map + collect.
  The most common anti-pattern.

- **Linear search** — `for x in items { if cond { return x
  } }` → find / position / any. Eliminates manual index
  tracking.

- **Reverse search** — `for i in (0..n).rev() { if cond
  { return i } }` → rev + find. Same clarity, less code.

- **Flat mapping** — `for outer { for inner { push(...) }
  }` → flat_map + map + collect. Nested loops that just
  collect are direct translations.

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
