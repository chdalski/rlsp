---
paths:
  - "**/*.py"
---

# Python — Testing

Python testing patterns using pytest. For cross-language
testing principles, see `code-principles.md`.

## Pytest Framework

Use pytest as the test framework — its fixtures, parametrize,
and assertion introspection reduce test boilerplate:

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

## Fixtures

Fixtures provide reusable test setup — they compose cleanly
and handle teardown automatically:

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

## Parametrize

Use parametrize for data-driven tests — it avoids
duplicating test logic across similar cases:

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

## Property-Based Testing

Use Hypothesis for property-based testing — it finds edge
cases that manual test data misses:

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
