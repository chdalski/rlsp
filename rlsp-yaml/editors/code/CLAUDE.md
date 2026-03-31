# VS Code Extension — Project Context

## pnpm Store

Use the default pnpm store location. Do **not** add a `store-dir` override in `.npmrc`.

The default store (`~/.local/share/pnpm/store`) is the correct location — no project-level configuration is needed. Adding a `store-dir` override moves the store into the project tree, which wastes disk space and conflicts with any environment that already manages the store externally.

When creating a new extension (e.g., `rlsp-toml/editors/code/`), leave `.npmrc` absent or empty.

## TypeScript Strictness

`tsconfig.json` extends `@tsconfig/strictest` from the [`tsconfig/bases`](https://github.com/tsconfig/bases) package. This enables `strict: true` plus `exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`, `noPropertyAccessFromIndexSignature`, `noUnusedLocals`, `noUnusedParameters`, `noImplicitReturns`, `noFallthroughCasesInSwitch`, and more.

ESLint uses `strictTypeChecked` + `stylisticTypeChecked` from `typescript-eslint`.

This mirrors the Rust stance: `clippy::pedantic + nursery` with selected lints at `deny`. New extensions should adopt the same tsconfig and ESLint configuration.
