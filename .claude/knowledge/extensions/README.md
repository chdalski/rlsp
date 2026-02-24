# Project Rules

This directory holds project-specific rules that agents
**must follow**. All agents load all files in this
directory during startup. Unlike the base knowledge files
(which provide guidance), everything in this directory is
a **mandatory requirement**.

## How to Use

Create `.md` files in this directory with conventions
specific to your project. Keep files short and focused —
one topic per file. All agents read all extension files,
so avoid putting information here that only applies to
project setup (that belongs in the project's `CLAUDE.md`).

## Format

Each file should have a clear title and concise rules.
**Every statement is a requirement** — agents must follow
them, not treat them as suggestions. You can optionally
specify which agents the rule is most relevant to.

## Example

```markdown
# Rust Conventions

**Agents**: Developer, Reviewer

## Module Structure

- Use `module_name.rs` files, not `mod.rs` for modules.
- Re-export from the parent module with `pub use`.

## Test Placement

- Write unit tests as inline `#[cfg(test)]` modules in
  the same file as the code they test.
- Use `/tests/` only for integration tests that exercise
  multiple modules together.

## Error Handling

- Use `thiserror` for library errors, `anyhow` for
  application errors.
- Every public function that can fail returns `Result`.
```

## What Belongs Here vs. in CLAUDE.md

| Here (extensions)                    | Project CLAUDE.md              |
|--------------------------------------|--------------------------------|
| Coding conventions and style rules   | Build and run commands         |
| Architecture decisions               | Repository structure overview  |
| Testing placement and strategy       | CI/CD configuration            |
| Error handling patterns              | Environment setup              |
| Naming conventions                   | Dependency management          |
