# Design Principles

## Kent Beck's Four Rules of Simple Design

Fundamental principles for writing clean, maintainable code.
Apply in priority order. These rules work synergistically
with TDD.

### 1. Tests Pass

- All tests must pass
- The code must work correctly
- Highest priority rule -- never compromise working code
- If tests don't pass, fix them before applying other rules

Tests must cover edge cases like:

- Boundary conditions (empty collections, zero/negative IDs)
- Error conditions (I/O failures, invalid input)
- Domain-specific logic (date math, currency rounding)

### 2. Reveals Intent

- Code should clearly express what it does
- Use meaningful names for variables, functions, and types
- Structure code to be self-documenting
- Prefer explicit over clever code
- Comments should explain "why", not "what"

Use the type system to make invalid states unrepresentable.
Clear function signatures serve as documentation.

### 3. No Duplication (DRY)

- Don't repeat yourself
- Extract common functionality into reusable components
- Look for both obvious and conceptual duplication
- Knowledge should have a single, unambiguous representation

**See also**: [data.md](data.md) for Single Source of Truth.

### 4. Fewest Elements

- Minimize the number of types, functions, and code elements
- Remove unnecessary abstractions
- Keep it simple (KISS) -- don't over-engineer
- Only add complexity when it serves a clear purpose
- Don't implement features until needed (YAGNI)

## Application Guidelines

### Priority Order

- Apply rules in order: 1 -> 2 -> 3 -> 4
- Never violate a higher-priority rule for a lower one
- **If rule #3 conflicts with rule #2, choose clarity**
- If rule #4 conflicts with #2 or #3, choose the
  higher-priority rule

### When to Choose Clarity Over DRY

Duplication is acceptable when:

- The duplicated code serves different purposes
  (accidental similarity)
- Removing duplication would create unclear abstractions
- The coupling from sharing code is worse than duplication
- Domain concepts are similar but will likely diverge

## KISS -- Keep It Simple

- Choose the simplest solution that works
- Avoid unnecessary complexity in data structures,
  algorithms, and architecture
- If a solution is hard to explain, it's probably too complex
- Simple solutions are easier to test, debug, and maintain

## YAGNI -- You Aren't Gonna Need It

- Don't implement functionality until it is needed
- Don't design for hypothetical future requirements
- Remove unused code and dead abstractions
- Premature generalization is a form of over-engineering

## SOLID Principles

### Single Responsibility (SRP)

A module should have one, and only one, reason to change.

### Open-Closed (OCP)

Software entities should be open for extension but closed
for modification.

### Liskov Substitution (LSP)

Subtypes must be substitutable for their base types without
altering program correctness.

### Interface Segregation (ISP)

No client should be forced to depend on methods it does
not use. Prefer small, focused interfaces.

### Dependency Inversion (DIP)

Depend on abstractions, not concretions. High-level modules
should not depend on low-level modules.

## Examples

### Rule 2: Reveals Intent

```pseudocode
// Bad -- unclear types and names
function get(id) -> list of strings

// Good -- clear domain types and intent
function fetch_templates(
    group: DepartmentGroup
) -> Result<list of Template, RepositoryError>

// Better -- use wrapper types for domain clarity
type OrderId = int
type DepartmentGroup = string

function fetch_templates(
    group: DepartmentGroup
) -> Result<list of Template, RepositoryError>
```

### Rule 3: No Duplication

```pseudocode
// Bad -- duplicated date adjustment logic
function adjust_start_dates(order, offset):
    order.start_date = order.start_date + offset
    order.end_date = order.end_date + offset

function adjust_delivery_dates(order, offset):
    order.delivery_start = order.delivery_start + offset
    order.delivery_end = order.delivery_end + offset

// Good -- extract common pattern
function shift_date(date, offset) -> date:
    return date + offset
```

### Rule 4: Fewest Elements

```pseudocode
// Bad -- unnecessary abstraction for single implementation
interface DateFormatter:
    format(date) -> string

class StorageDateFormatter implements DateFormatter:
    format(date) -> string:
        return date.format("YYYY_MM_DD")

// Good -- simple function until multiple implementations
// are needed
function format_storage_date(date) -> string:
    return date.format("YYYY_MM_DD")
```

```pseudocode
// Bad -- unnecessary builder for simple structure
class ConfigBuilder:
    db_host: optional string
    port: optional int
    ...

// Good -- simple constructor
function create_config(db_host, port) -> Config
// Only add builder when you have many optional fields
```

## Red Flags

- Code that's hard to name (violates Rule #2)
- Copy-paste programming (violates Rule #3)
- Premature abstractions (violates Rule #4)
- Complex conditional logic that could be simplified
- Deep inheritance or trait/interface hierarchies
  (violates Rule #4)
- Overly complex generic type constraints
  (violates Rule #2)
- Using raw primitives instead of domain-specific types
- Panic/crash in production code paths (violates Rule #1)
- Unnecessary data copying or allocation

## Testing and Simple Design

### What Makes Tests Support Simple Design

Good tests should:

- **Support Rule #1**: Be reliable and catch regressions
- **Support Rule #2**: Serve as documentation of intent
- **Support Rule #3**: Encourage DRY implementation through
  repetitive test setup
- **Support Rule #4**: Push toward simpler APIs that are
  easy to test

### Test Quality

```pseudocode
// Bad -- unclear test intent
test test1():
    x = OrderId.new(-1)
    assert x.is_error()

// Good -- clear intent
test order_id_rejects_negative_value():
    result = OrderId.new(-1)
    assert result is Error(NegativeIdError)
```

## Remember

- **Simple does not mean Easy**: Simple means "not complex"
- **Priority matters**: Working code trumps everything
- **Clarity over cleverness**: Hard to understand is not
  simple
- **Rules work together**: Applying them in order leads to
  emergent design
- **Context matters**: Correctness and clarity are paramount
- **Refactor continuously**: Simple design emerges through
  disciplined refactoring
