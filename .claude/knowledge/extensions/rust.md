# Rust Conventions

**Agents**: Developer, Test Engineer, Code Reviewer

## Module Structure

- Use `module_name.rs` files, not `mod.rs` for modules.
- Re-export from the parent module with `pub use`.

## Test Placement

- Write unit tests as inline `#[cfg(test)]` modules in
  the same file as the code they test.
- Use `/tests/` only for integration tests that exercise
  multiple modules together.

## Linting

- Use Clippy with pedantic and nursery lints enabled.
  Do NOT enable `clippy::restriction` as a group — it
  produces 80+ false positives and is not meant to be
  enabled wholesale. Add specific restriction lints
  individually if needed.

```bash
cargo clippy --all -- -W clippy::all -W clippy::pedantic -W clippy::nursery -D warnings
```
