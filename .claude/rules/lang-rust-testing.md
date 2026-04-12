---
paths:
  - "**/*.rs"
---

# Rust Testing

## Framework and Tools

- `cargo test` for unit and integration tests
- `proptest` for property-based testing
- `mockall` for mocking trait implementations

## Test Organization

Rust has three test locations, each with a distinct purpose
— choosing the right one keeps tests focused and avoids
over-mocking:

- **Inline `#[cfg(test)]` modules** — unit tests inside the
  source file; they have access to private items, which is
  the only way to test internal invariants directly
- **`tests/` directory** — integration tests compiled as a
  separate crate; they can only access public API, which
  makes them true black-box regression tests
- **Doc tests (`///`)** — code examples in documentation
  comments that `cargo test` runs as tests; use them for
  happy-path demonstrations so documentation and behaviour
  stay in sync

```rust
/// Returns a validated customer ID.
///
/// ```
/// # use mylib::CustomerId;
/// let id = CustomerId::new(42).unwrap();
/// assert_eq!(id.value(), 42);
/// ```
pub fn new(value: i64) -> Result<Self, ValidationError> { ... }
```

## Design for Testability

Keep business logic free of direct I/O — functions that call
`println!` embed an effect that tests cannot observe or
redirect without capturing stdout. Two complementary
patterns eliminate this:

**Accept `impl Write` for output** so tests pass a
`Vec<u8>` as an in-memory sink:

```rust
// Hard to test — output is embedded
fn print_report(orders: &[Order]) {
    for o in orders {
        println!("{}: {}", o.id, o.status);
    }
}

// Testable — caller injects the sink
fn write_report(
    orders: &[Order],
    out: &mut impl Write,
) -> io::Result<()> {
    for o in orders {
        writeln!(out, "{}: {}", o.id, o.status)?;
    }
    Ok(())
}

// In tests
let mut buf = Vec::new();
write_report(&orders, &mut buf)?;
assert_eq!(String::from_utf8(buf)?, "1: pending\n");
```

**Return action enums for decisions** so the caller executes
the effect and tests verify only the decision:

```rust
enum PricingDecision {
    ApplyDiscount { percent: u8 },
    NoDiscount,
}

fn evaluate_order(order: &Order) -> PricingDecision {
    if order.total > 100 {
        PricingDecision::ApplyDiscount { percent: 10 }
    } else {
        PricingDecision::NoDiscount
    }
}

#[test]
fn high_value_order_gets_discount() {
    let order = Order::with_total(150);
    assert!(matches!(
        evaluate_order(&order),
        PricingDecision::ApplyDiscount { percent: 10 }
    ));
}
```

## Test Structure

Use descriptive names with Arrange-Act-Assert pattern:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_id_rejects_zero_value() {
        let result = CustomerId::new(0);

        assert!(matches!(
            result,
            Err(ValidationError::NonPositiveId)
        ));
    }

    #[test]
    fn order_prevents_adding_items_when_confirmed() {
        let mut order = Order::confirmed(sample_id());

        let result = order.add_item(sample_item());

        assert!(matches!(
            result,
            Err(DomainError::OrderNotEditable)
        ));
    }
}
```

## Parameterized Tests

When consolidating standalone `#[test]` functions into a
parameterized test (via `rstest`, `test-case`, or any
macro that collapses multiple scenarios into one function),
preserve the intent of each original test name using the
framework's named-case syntax.

The original function name documents *what behavior* the
case exercises. A bare case line with only input and
expected values loses that documentation — a failing case
then shows raw values, and the developer cannot tell what
scenario broke without reading production code.

For rstest, use the `#[case::name]` syntax — the name
becomes part of the test identity, appears in test output
and failure messages, and is grep-able:

```rust
// Bad — intent lost
#[rstest]
#[case("abc   ", "abc")]
#[case("", "")]
#[case("a:b", "a:b")]
fn scan_plain(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(scan_plain_line_block(input), expected);
}

// Good — intent preserved via named cases
#[rstest]
#[case::trailing_whitespace_excluded("abc   ", "abc")]
#[case::empty_input_returns_empty("", "")]
#[case::colon_mid_word_is_content("a:b", "a:b")]
fn scan_plain(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(scan_plain_line_block(input), expected);
}
```

When a group has mixed assertion shapes (`assert_eq!`,
`matches!`, span-field checks), split into multiple
parameterized functions named after their assertion shape
(e.g. `scalar_cases_eq`, `scalar_cases_cow`). Do not
create helpers that normalize diverse outputs into one
unified return type — keep assertion shape obvious at the
test site.

## Property-Based Testing

Use proptest to verify properties over random inputs — it
finds edge cases that manual test data misses:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn customer_id_always_positive(id in 1..i64::MAX) {
        let result = CustomerId::new(id);
        assert!(result.is_ok());
    }

    #[test]
    fn customer_id_rejects_non_positive(id in i64::MIN..=0) {
        let result = CustomerId::new(id);
        assert!(result.is_err());
    }
}
```
