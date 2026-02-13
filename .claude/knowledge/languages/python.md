# Python Language Extension

> Extends base principles from `knowledge/base/principles.md`

## Philosophy

Python emphasizes readability and simplicity. "There should
be one obvious way to do it." Write Pythonic code that
leverages the language's strengths: list comprehensions,
generators, context managers, and duck typing. Use type
hints for documentation and static analysis while embracing
Python's dynamic nature pragmatically.

## Pythonic Patterns

### List Comprehensions and Generators

Prefer comprehensions over manual loops for transformations:

```python
# Imperative (avoid)
results = []
for user in users:
    if user.is_active:
        results.append(user.name.upper())

# Pythonic (preferred)
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

### Context Managers

Use context managers for resource management:

```python
# Bad - manual cleanup
f = open("data.txt")
try:
    data = f.read()
finally:
    f.close()

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

## Type Hints

### Basic Type Annotations

```python
from typing import Protocol, TypeVar, Generic
from collections.abc import Sequence, Mapping

def find_user(
    user_id: int,
    repo: UserRepository,
) -> User | None:
    return repo.find(user_id)

def process_items(
    items: Sequence[Item],
) -> list[ProcessedItem]:
    return [process(item) for item in items]
```

### Protocols for Structural Typing

Use `Protocol` instead of abstract base classes when
you want structural (duck) typing:

```python
from typing import Protocol, runtime_checkable

@runtime_checkable
class Repository(Protocol):
    def find(self, id: int) -> object | None: ...
    def save(self, entity: object) -> None: ...

# Any class with find() and save() satisfies this
class PostgresUserRepo:
    def find(self, id: int) -> User | None:
        ...

    def save(self, entity: User) -> None:
        ...
```

### Generics

```python
from typing import TypeVar, Generic

T = TypeVar("T")
E = TypeVar("E")

class Result(Generic[T, E]):
    """Rust-inspired Result type."""

    def __init__(
        self,
        value: T | None = None,
        error: E | None = None,
    ) -> None:
        self._value = value
        self._error = error

    @classmethod
    def ok(cls, value: T) -> "Result[T, E]":
        return cls(value=value)

    @classmethod
    def err(cls, error: E) -> "Result[T, E]":
        return cls(error=error)
```

### Discriminated Unions

```python
from dataclasses import dataclass
from typing import Union

@dataclass(frozen=True)
class Pending:
    created_at: datetime

@dataclass(frozen=True)
class Confirmed:
    confirmed_at: datetime

@dataclass(frozen=True)
class Shipped:
    tracking: str

OrderStatus = Union[Pending, Confirmed, Shipped]

def handle_status(status: OrderStatus) -> str:
    match status:
        case Pending(created_at=dt):
            return f"Pending since {dt}"
        case Confirmed(confirmed_at=dt):
            return f"Confirmed at {dt}"
        case Shipped(tracking=t):
            return f"Shipped: {t}"
```

## Functional Patterns

### Map, Filter, and Functools

```python
from functools import reduce, partial
from itertools import chain, groupby

# Prefer comprehensions for simple cases
squares = [x ** 2 for x in numbers]

# Use functools for composition
from functools import reduce

def compose(*fns):
    def composed(x):
        return reduce(
            lambda acc, f: f(acc), reversed(fns), x
        )
    return composed

process = compose(validate, enrich, transform)
result = process(raw_input)
```

### Immutability

```python
from dataclasses import dataclass

# Use frozen dataclasses for value objects
@dataclass(frozen=True)
class Point:
    x: float
    y: float

    def translate(self, dx: float, dy: float) -> "Point":
        return Point(self.x + dx, self.y + dy)

# Use tuples for immutable sequences
coordinates = (1.0, 2.0, 3.0)
```

### Itertools for Complex Transformations

```python
from itertools import (
    chain, groupby, islice, product, starmap
)

# Chain multiple iterables
all_items = chain(list_a, list_b, list_c)

# Group by key
sorted_data = sorted(data, key=lambda x: x.category)
for category, items in groupby(
    sorted_data, key=lambda x: x.category
):
    process_group(category, list(items))
```

## Error Handling

### Custom Exceptions

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
Permission) over LBYL (Look Before You Leap):

```python
# LBYL (avoid in Python)
if key in dictionary:
    value = dictionary[key]
else:
    value = default

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

## Testing

### Pytest Framework

```python
import pytest

class TestCustomerId:
    def test_rejects_zero(self):
        with pytest.raises(ValidationError):
            CustomerId(0)

    def test_rejects_negative(self):
        with pytest.raises(ValidationError):
            CustomerId(-1)

    def test_accepts_positive(self):
        cid = CustomerId(42)
        assert cid.value == 42
```

### Fixtures

```python
import pytest

@pytest.fixture
def sample_order():
    return Order(
        id=OrderId(1),
        items=[Item(name="Widget", quantity=2)],
        status=Pending(created_at=datetime.now()),
    )

@pytest.fixture
def mock_repo(mocker):
    return mocker.create_autospec(OrderRepository)

def test_find_order(mock_repo, sample_order):
    mock_repo.find.return_value = sample_order

    result = service.find_order(OrderId(1), mock_repo)

    assert result == sample_order
    mock_repo.find.assert_called_once_with(OrderId(1))
```

### Parametrize

```python
@pytest.mark.parametrize(
    "input_val,expected",
    [
        (1, True),
        (0, False),
        (-1, False),
        (100, True),
    ],
)
def test_customer_id_validation(input_val, expected):
    if expected:
        assert CustomerId(input_val).value == input_val
    else:
        with pytest.raises(ValidationError):
            CustomerId(input_val)
```

### Hypothesis for Property-Based Testing

```python
from hypothesis import given
from hypothesis import strategies as st

@given(st.integers(min_value=1))
def test_customer_id_always_valid_for_positive(value):
    cid = CustomerId(value)
    assert cid.value == value

@given(st.integers(max_value=0))
def test_customer_id_rejects_non_positive(value):
    with pytest.raises(ValidationError):
        CustomerId(value)
```

## Code Style and Tooling

### Required Tools

- **Ruff** for linting and formatting (replaces flake8,
  isort, black)
- **mypy** or **pyright** for type checking
- **pytest** for testing

### Project Structure (src Layout)

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

## Workflow Details

### Clean Builds

Remove `__pycache__/`, `.pyc` files, and `.mypy_cache/`
before quality checks to avoid stale bytecode or cached
type-checking results.

### Build Tool Commands

- `ruff format .` — format code
- `ruff check .` — lint
- `mypy .` / `pyright` — type check
- `pytest` — run tests
- `find . -type d -name __pycache__ -exec rm -rf {} +` —
  remove cached bytecode

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

### Mutable Default Argument

```python
# Bug - shared mutable default
def add_item(item, items=[]):
    items.append(item)
    return items

# Fix - use None sentinel
def add_item(item, items=None):
    if items is None:
        items = []
    items.append(item)
    return items
```

### Late Binding in Closures

```python
# Bug - all functions return 4
fns = [lambda: i for i in range(5)]

# Fix - capture value with default arg
fns = [lambda i=i: i for i in range(5)]
```
