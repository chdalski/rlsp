---
test-name: block-scalar-whitespace-only-folded
category: block-scalar
idempotent: true
---

# Test: Folded Block Scalar With Spaces-Only Content Is Handled By The Guard

The formatter's whitespace-only guard applies to both literal (`|`) and folded
(`>`) block scalars. When a folded block scalar's decoded value contains a
non-empty line that starts with a space and consists entirely of whitespace,
the formatter falls back to double-quoted output.

The parser normalises folded scalar input into a decoded value string. A folded
scalar value consisting solely of `" \n"` (space plus newline) would be emitted
as `" \n"` in double-quoted form. This fixture verifies idempotency of that
double-quoted form — the formatter does not attempt to re-encode it as a block
scalar.

Note: a raw `key: >` block scalar whose sole content line is all-spaces is
rejected by the parser (the blank-line indentation constraint fires at parse
time, before the formatter runs). This fixture therefore starts from the stable
double-quoted output form that the formatter produces for such values.

## Test-Document

```yaml
key: " \n"
```
