# Code quality improvements for rlsp-yaml-parser

## chars.rs:

- Remove the two lines and remove the dead code as well: 
```rust
// Functions defined here will be used by scanner/lexer in later tasks.
#![allow(dead_code)]
``` 

## lexer.rs

- Move the function into the test module where it belongs (and remove the `#[cfg(test)]` as a result of that change as well): 
```rust
/// True when `line` is blank, comment-only, or a directive (`%`-prefixed).
///
/// Directive lines (`%YAML`, `%TAG`, and unknown `%` directives) are
/// stream-level metadata that precede `---`.  This predicate is only correct
/// to use in the between-documents context; inside a document body `%`-prefixed
/// lines are content and must be handled by [`is_blank_or_comment`] instead.
///
/// Used only in tests to verify the `BetweenDocs` predicate.
#[cfg(test)]
fn is_directive_or_blank_or_comment(line: &Line<'_>) -> bool {
    if is_blank_or_comment(line) {
        return true;
    }
    let trimmed = line.content.trim_start_matches([' ', '\t']);
    trimmed.starts_with('%')
}
```
- We've split the lexer into submodules - tests that test functionality in a submodule should be in that submodule as well, i.e. the following should be in plain.rs:
```rust
    // SPF-1: plain word terminates at `]`
    #[test]
    fn flow_plain_terminates_at_close_bracket() {
        assert_eq!(scan_plain_line_flow("abc]rest"), "abc");
    }
```


## lib.rs
- Is to long, decide if it makes sense to split into multiple files and if so, how (also make sure the tests are migrated alongside their functions like with the plain.rs example above)
- #[allow(clippy::struct_excessive_bools)] <- discuss if and how we could remove it

## loader.rs
- Is to long, decide if it makes sense to split into multiple files and if so, how (also make sure the tests are migrated alongside their functions like with the plain.rs example above)

## README.md
- Currently not in place - should be in the todo-queue
- Should include AI Note

## rlsp-yaml-parser/docs/benchmarks.md
- Should remove the references to previous versions