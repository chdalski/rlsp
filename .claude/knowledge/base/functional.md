# Functional Programming Principles

These guidelines define functional programming (FP)
principles that apply across languages. Follow these
principles to write clean, composable, and maintainable
code.

## Core Principles

### Functions as First-Class Citizens

- Pass functions as arguments to enable abstraction
- Return functions from other functions
- Store functions in data structures when appropriate
- Use higher-order functions (`map`, `filter`, `reduce`)
  over explicit loops
- Use dynamic dispatch for runtime polymorphism when needed

```pseudocode
// Higher-order function accepting a transformation
function transform_dates(dates, transform_fn):
    return dates.map(transform_fn)

// Usage with anonymous function
shifted = transform_dates(dates, d => d + 7 days)
```

### Deterministic Functions

- Same inputs must always produce the same output
- Avoid dependencies on mutable global state
- Avoid time-dependent or random operations in pure
  functions
- Make external dependencies explicit through parameters
- Use pure functions for business logic and calculations

```pseudocode
// Good -- deterministic
function adjust_date(original, reference, target):
    return target + (original - reference)

// Bad -- depends on global mutable state and current time
function adjust_date(original):
    reference = GLOBAL_CONFIG.reference_date
    return original + (now() - reference)
```

### Avoid Side Effects

- Separate pure computation from I/O operations
- Mark functions with side effects clearly (naming, types)
- Keep side effects at the boundaries (adapters, I/O layer)
- Return values instead of mutating state

```pseudocode
// Pure function (preferred for business logic)
function apply_offset(date, offset):
    return date + offset

// Function with side effects (clearly named)
function store_and_log_record(repository, record):
    repository.store(record)
    log.info("Record stored: " + record.id)
```

### Immutable Data

- Prefer immutable bindings by default
- Use owned values and transformations over in-place
  mutation
- Copy when necessary rather than sharing mutable
  references
- Consider persistent/immutable data structures for
  complex state

```pseudocode
// Good -- return new value instead of mutating
function with_adjusted_date(record, offset):
    updated = copy(record)
    updated.date = record.date + offset
    return updated

// Better -- with builder/copy-on-write pattern
function with_adjusted_date(record, offset):
    return record
        .with_date(record.date + offset)
        .with_end_date(record.end_date + offset)
```

### Declarative Style

- Express what to compute, not how to compute it
- Use collection pipelines over manual loops
- Leverage pattern matching for control flow
- Use method chaining for data transformations
- Prefer expressions over statements

```pseudocode
// Imperative (avoid)
matching = []
for item in items:
    if item.group == target:
        matching.append(item)

// Declarative (preferred)
matching = items
    .filter(item => item.group == target)
    .to_list()
```

### Function Composition

- Build complex operations from simple, composable
  functions
- Create small, focused functions that do one thing well
- Combine functions using pipelines and combinators

```pseudocode
function validate(request) -> optional request
function fetch_template(request) -> template
function copy_record(template, target_date) -> record

// Composed pipeline
function process_copy(request):
    return validate(request)
        .map(fetch_template)
        .map(t => copy_record(t, target_date))
```

## Practical Guidelines

- **Balance pragmatism with principles**: Use mutation when
  it is clearer or more performant
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
