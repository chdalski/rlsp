---
paths:
  - "**/*.py"
---

# Python — Type System and Functional Patterns

Python type system and functional patterns for structuring
code. For cross-language principles, see `functional-style.md`.

## Type Hints

### Basic Type Annotations

Type hints catch bugs at static analysis time and serve as
machine-checked documentation:

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

Use `Protocol` instead of abstract base classes when you
want structural (duck) typing — any class with matching
methods satisfies the protocol without explicit
inheritance:

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

Use frozen dataclasses with `match` for exhaustive state
handling — the type checker warns about unhandled variants:

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

### Comprehensions and Functools

Prefer comprehensions for simple cases — they're more
readable than `map`/`filter` in Python:

```python
from functools import reduce, partial
from itertools import chain, groupby

squares = [x ** 2 for x in numbers]

# Use functools for composition
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

Use frozen dataclasses for value objects — immutable
objects are safer to share across threads and easier to
reason about:

```python
from dataclasses import dataclass

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
