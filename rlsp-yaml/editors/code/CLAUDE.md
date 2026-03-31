# VS Code Extension — Project Context

## pnpm Store

Use the default pnpm store location. Do **not** add a `store-dir` override in `.npmrc`.

The devcontainer mounts a named volume at `~/.local/share/pnpm/store` (the pnpm default). Adding a `store-dir` override creates a mismatch — the override resolves to a path inside the project tree, not the volume. The volume is already persistent across rebuilds; no project-level config is needed.

When creating a new extension (e.g., `rlsp-toml/editors/code/`), leave `.npmrc` absent or empty.

## TypeScript Strictness

`tsconfig.json` extends `@tsconfig/strictest` from the [`tsconfig/bases`](https://github.com/tsconfig/bases) package. This enables `strict: true` plus `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`, `noPropertyAccessFromIndexSignature`, `noUnusedLocals`, `noUnusedParameters`, `noImplicitReturns`, `noFallthroughCasesInSwitch`, and more.

ESLint uses `strictTypeChecked` + `stylisticTypeChecked` from `typescript-eslint`.

This mirrors the Rust stance: `clippy::pedantic + nursery` with selected lints at `deny`. New extensions should adopt the same tsconfig and ESLint configuration.
