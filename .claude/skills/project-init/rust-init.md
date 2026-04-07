# Rust Project Initialization — Cargo Lint Configuration

Apply these lints to every `Cargo.toml` in the project when
Rust is detected. The correct section depends on whether the
file is a workspace root or a crate manifest — using the wrong
section is a silent error that Cargo ignores without warning.

## Crate `Cargo.toml` (has `[package]`)

If the crate has `lints.workspace = true`, skip it — the
workspace definition already covers it. Otherwise add:

```toml
[lints.clippy]
# https://github.com/rust-lang/cargo/issues/12918
all = { level = "warn", priority = -1 }      # -W clippy::all (enabled by default, but good to be explicit)
pedantic = { level = "warn", priority = -1 } # -W clippy::pedantic
nursery = { level = "warn", priority = -1 }  # -W clippy::nursery
indexing_slicing = "deny"                    # panics on out-of-bounds — use .get() instead
fallible_impl_from = "deny"                  # From impls that can panic — use TryFrom
wildcard_enum_match_arm = "deny"             # silently ignores new variants when enum grows
unneeded_field_pattern = "deny"              # dead pattern arms that hide refactoring bugs
fn_params_excessive_bools = "deny"           # boolean params are easy to swap — use enums
must_use_candidate = "deny"                  # functions whose return value should not be ignored
panic = "deny"                               # explicit panic!() — use proper error handling
expect_used = "deny"                         # panics with a message — use proper error handling
unwrap_used = "deny"                         # panics on None/Err — use proper error handling

[lints.rust]
warnings = "deny" # -D warnings
```

## Workspace `Cargo.toml` (has `[workspace]`)

```toml
[workspace.lints.clippy]
# https://github.com/rust-lang/cargo/issues/12918
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
indexing_slicing = "deny"
fallible_impl_from = "deny"
wildcard_enum_match_arm = "deny"
unneeded_field_pattern = "deny"
fn_params_excessive_bools = "deny"
must_use_candidate = "deny"
panic = "deny"
expect_used = "deny"
unwrap_used = "deny"

[workspace.lints.rust]
warnings = "deny" # -D warnings
```

Member crates inherit these by adding `lints.workspace = true`
to their own `Cargo.toml`. Prefer workspace-level lints in
multi-crate projects — they enforce consistency and avoid
duplicating config across crates.

## Merging

If a `Cargo.toml` already has a lints section, merge these
entries into it. If any lint conflicts with an existing entry,
keep the stricter setting (`"deny"` over `"warn"` over `"allow"`).
