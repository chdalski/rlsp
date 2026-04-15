---
test-name: block-scalar-whitespace-only-mixed-whitespace
category: block-scalar
---

# Test: Literal Block Scalar With Mixed Whitespace Content Falls Back To Quoted

When a literal block scalar has a decoded value whose non-empty lines consist
of spaces followed by tabs (or any combination of spaces and tabs starting with
a space), the formatter must fall back to a double-quoted scalar.

The YAML blank-line constraint applies to lines that start with a space and
contain only whitespace. After the formatter's indentation is applied, such a
line would have more leading spaces than the declared indent level, causing the
re-parser to reject the output.

A line starting with a tab is safe and does NOT trigger the fallback (the tab
is treated as a non-blank content character). Only lines starting with a space
and containing exclusively whitespace characters (spaces and/or tabs) trigger
the guard.

## Test-Document

```yaml
key: |
  	  
```

## Expected-Document

```yaml
key: " \t  \n"
```
