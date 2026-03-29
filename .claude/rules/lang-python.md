---
paths:
  - "**/*.py"
---

# Python

Python emphasizes readability and simplicity. "There should
be one obvious way to do it." Write Pythonic code that
leverages the language's strengths: list comprehensions,
generators, context managers, and duck typing. Use type
hints for documentation and static analysis while embracing
Python's dynamic nature pragmatically.

## Pythonic Patterns

### List Comprehensions and Generators

Use comprehensions instead of manual accumulate-in-loop
patterns when the criteria in `functional-style.md` are
met (readability, less code, no manual index math, lower
complexity). In Python, comprehensions are the primary
declarative alternative — they are more readable than
`map()`/`filter()` for most cases.

**Collect-and-append** — the most common anti-pattern.
Always refactor:

```python
# Anti-pattern — mutable accumulator + loop
results = []
for user in users:
    if user.is_active:
        results.append(user.name.upper())

# Refactored — comprehension
results = [
    user.name.upper()
    for user in users
    if user.is_active
]

# Generator for lazy evaluation (large datasets)
active_names = (
    user.name.upper()
    for user in users
    if user.is_active
)
```

**Linear search** — loops that scan for a match. Use
`next()` with a generator expression:

```python
# Anti-pattern — manual search loop
result = None
for user in users:
    if user.email == target:
        result = user
        break

# Refactored
result = next(
    (u for u in users if u.email == target), None
)
```

**When loops are correct in Python:**

- **Async iteration with multiple `await` points** — an
  `async for` loop with `await` and `try`/`except` per
  iteration is clearer than chaining async generators
  through `aiostream` combinators.
- **Comprehensions with side effects** — if the loop body
  does I/O or mutates external state, a comprehension
  hides the side effect. Keep the loop and make the effect
  visible.
- **Multi-line conditions with early exit** — when the
  loop body has complex `break`/`continue` logic tied to
  different state, a comprehension would require nesting
  that hurts readability.

See `functional-style.md` for the full decision criteria.

### Context Managers

Use context managers for resource management — they
guarantee cleanup even when exceptions occur:

```python
# Good - context manager
with open("data.txt") as f:
    data = f.read()

# Custom context manager
from contextlib import contextmanager

@contextmanager
def database_transaction(conn):
    try:
        yield conn.cursor()
        conn.commit()
    except Exception:
        conn.rollback()
        raise
```

### Unpacking and Destructuring

```python
# Tuple unpacking
first, *rest = items
x, y, z = point

# Dictionary unpacking
defaults = {"timeout": 30, "retries": 3}
config = {**defaults, "timeout": 60}
```

## Error Handling

### Custom Exceptions

Define domain-specific exceptions — generic `Exception`
catches hide bugs and make error handling imprecise:

```python
class DomainError(Exception):
    """Base for all domain errors."""

class NotFoundError(DomainError):
    def __init__(self, entity: str, id: int) -> None:
        self.entity = entity
        self.id = id
        super().__init__(f"{entity} not found: {id}")

class ValidationError(DomainError):
    def __init__(self, field: str, reason: str) -> None:
        self.field = field
        self.reason = reason
        super().__init__(f"{field}: {reason}")
```

### EAFP vs LBYL

Python favors EAFP (Easier to Ask Forgiveness than
Permission) — it avoids race conditions between check and
use:

```python
# EAFP (preferred)
try:
    value = dictionary[key]
except KeyError:
    value = default

# Even better - use built-in methods
value = dictionary.get(key, default)
```

### Structured Error Handling

```python
def process_order(order_data: dict) -> Order:
    try:
        validated = validate(order_data)
        return create_order(validated)
    except ValidationError as e:
        logger.warning("Validation failed: %s", e)
        raise
    except DatabaseError as e:
        logger.error("Database error: %s", e)
        raise ServiceError("Failed to create order") from e
```

## Code Style and Tooling

### Required Tools

- **Ruff** for linting and formatting (replaces flake8,
  isort, black) — single tool reduces config sprawl.
  Remove `__pycache__/`, `.pyc`, and `.mypy_cache/` before
  quality checks — stale bytecode can mask import errors.
- **mypy** or **pyright** for type checking
- **pytest** for testing

### Project Structure (src Layout)

The src layout prevents accidental imports from the project
root that work locally but fail when installed:

```text
project/
  src/
    mypackage/
      __init__.py
      orders/
        __init__.py
        order.py
        order_service.py
      users/
        __init__.py
        user.py
  tests/
    orders/
      test_order.py
      test_order_service.py
    users/
      test_user.py
  pyproject.toml
```

### Style Guidelines

- Follow PEP 8 naming conventions
- Functions and variables: `snake_case`
- Classes: `PascalCase`
- Constants: `UPPER_SNAKE_CASE`
- Keep functions focused and short
- Use docstrings for public APIs (Google style)

## Common Pitfalls

| Pitfall | Why It's Bad | Fix |
|---|---|---|
| Mutable defaults | Shared across calls | Use `None` + create inside |
| Late binding closures | Captures variable ref | Use default args |
| Bare `except:` | Catches everything | Catch specific exceptions |
| Global mutable state | Hard to test/reason | Dependency injection |
| `isinstance` chains | Not extensible | Use polymorphism/protocols |
| Ignoring GIL | No true parallelism | Use multiprocessing/async |
| String formatting with % | Outdated, error-prone | Use f-strings |
| Deep inheritance | Rigid hierarchies | Composition + protocols |
| Accumulate-in-loop | Higher code mass, mutable state | Use comprehensions or `next()` |
