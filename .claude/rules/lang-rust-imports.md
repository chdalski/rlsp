---
paths:
  - "**/*.rs"
---

# Rust Import Placement

Every `use` and `mod` declaration belongs at the top of its
enclosing module or block, before any other item. Scattering
imports through a file hides the dependency graph — a reader
should be able to survey what a module depends on without
scrolling past unrelated code.

Clippy's `items_after_statements` catches only one narrow
case (in-function `use` after a statement in the same block).
This rule covers what clippy misses — module scope, inline
`mod` blocks, test modules, and `use` at the top of a
function body. Enforcement is reviewer-driven; no custom
lint is maintained.

## Module-Scope Placement (Two Tiers)

The order within a file's header block depends on the file's
role. Crate roots define the module tree; sub-modules
contain implementation that imports from elsewhere.

### Crate roots — `mod` → `pub use` → `use` → items

A file is a **crate root** when it is one of:

- `<crate>/src/lib.rs`
- `<crate>/src/main.rs`
- `<crate>/src/bin/*.rs`
- `<crate>/tests/<name>.rs` (each integration test is its
  own crate)
- `<crate>/tests/<dir>/main.rs` (multi-file integration
  test crate)
- `<crate>/benches/*.rs` (each bench is its own crate)

`mod` declarations come first — the reader sees the module
map before anything else. Next come `pub use` re-exports
that shape the public API, then regular `use` imports, then
items.

```rust
// 1. mod declarations — the module tree
mod error;
pub mod loader;
pub mod node;

// 2. pub use — re-exports that shape the public API
pub use error::Error;
pub use node::{Document, Node};

// 3. use — internal imports consumed by items below
use std::collections::VecDeque;

use serde_json::Value;

// 4. items
pub fn parse(input: &str) -> Result<Document, Error> {
    // ...
}
```

### Sub-modules — `use` → `mod` → items

Every other `.rs` file is a **sub-module**. Imports come
first, then any nested child modules, then items.

```rust
// 1. use — std, external, crate (rustfmt groups these)
use std::borrow::Cow;

use serde_json::Value;

use crate::error::Error;

// 2. mod — nested child modules declared by this file
mod helpers;

// 3. items
pub fn validate(value: &Value) -> Result<(), Error> {
    // ...
}
```

Rustfmt handles alphabetical ordering inside each `use`
group (`reorder_imports`, on by default). This rule only
governs the relative order of the groups themselves.

## Block-Scope Placement

The same top-of-block rule applies recursively inside every
block — inline `mod X { ... }` definitions, `#[cfg(test)]
mod tests { ... }`, and function bodies. Imports precede
any non-import statement or item in the enclosing block.

### Test modules

Inside `#[cfg(test)] mod tests { ... }`, `use super::*;`
and any other imports go at the top of the block, before
any `#[test] fn`. A `use` below a test function is a
violation, even though the test module itself is
idiomatically placed at the bottom of the file.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn rejects_empty_input() { /* ... */ }
}
```

## Function-Body `use` — Allowed Exceptions

A `use` inside a function body is acceptable only in these
three patterns. Every other function-body `use` must be
hoisted to the module's `use` block or deleted.

### 1. Variant glob

`use Foo::*;` at the top of a function body to shorten
match arms. Only valid when the body actually uses the
unqualified variant names — if every reference is
`Foo::Bar`, the import is dead and should be removed.

```rust
fn classify(status: OrderStatus) -> &'static str {
    use OrderStatus::*;
    match status {
        Pending   => "waiting",
        Confirmed => "ready",
        Shipped   => "in transit",
        Delivered => "done",
    }
}
```

### 2. Name-collision resolution

When two paths export the same name and hoisting would
force `as` aliases at every other call site, a local `use`
resolves the collision without polluting module scope. Add
a one-line comment explaining the collision if it is not
obvious from context.

```rust
fn render(doc: &serde_yaml::Value) -> String {
    // Value also exists in serde_json at module scope;
    // local use avoids aliasing both imports file-wide.
    use serde_yaml::Value;
    match doc {
        Value::String(s) => s.clone(),
        _ => String::new(),
    }
}
```

### 3. `#[cfg]`-gated path

When the `use` is only valid or needed under a specific
cfg and hoisting would require `#[cfg_attr]` gymnastics
on the module-level import, a cfg-gated local `use` is
acceptable. Add a one-line comment stating the cfg reason.

## Anti-Pattern: Dead Local `use`

If every reference to the imported name in the block is
fully-qualified, the local `use` is dead — it does nothing.
Delete it, or hoist it to the module's `use` block and
drop the prefix at the call site.

```rust
fn validate(node: &Node) -> bool {
    // DEAD: body uses fully-qualified ScalarStyle::Plain.
    use crate::parser::ScalarStyle;

    matches!(node, Node::Scalar {
        style: crate::parser::ScalarStyle::Plain, ..
    })
}
```

## Why This Matters

- **Dependency visibility** — a reader opening a file
  should see its dependencies in the first screen. Imports
  scattered through function bodies hide what the module
  consumes.
- **Refactor safety** — when imports are in one place,
  renaming or removing a dependency touches one section.
  Scattered imports mean a rename misses hidden call sites
  inside function bodies.
- **Review signal** — reviewers verify the import set
  against the module's purpose at a glance. Hidden imports
  evade this check.
- **Idiomatic Rust** — the
  [Rust Style Guide](https://doc.rust-lang.org/nightly/style-guide/#imports)
  places imports after inner attributes and before items;
  this rule is the project-specific refinement for the
  crate-root vs sub-module distinction and for
  block-scope recursion.
