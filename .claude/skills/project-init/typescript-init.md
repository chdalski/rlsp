# TypeScript Project Initialization — Strictness Configuration

Apply these strictness settings to every TypeScript project
root detected during `/project-init`: maximum compiler
strictness via `@tsconfig/strictest` and maximum lint
strictness via `typescript-eslint` strict type-checked
configs.

## tsconfig.json

Add or ensure the `extends` field points to
`@tsconfig/strictest`:

```json
{
  "extends": "@tsconfig/strictest/tsconfig.json"
}
```

`@tsconfig/strictest` enables `strict: true` plus
`exactOptionalPropertyTypes`, `noUncheckedIndexedAccess`,
`noPropertyAccessFromIndexSignature`, `noUnusedLocals`,
`noUnusedParameters`, `noImplicitReturns`,
`noFallthroughCasesInSwitch`, and more.

### Merging

- If `tsconfig.json` exists and already extends
  `@tsconfig/strictest`, skip — nothing to do.
- If `tsconfig.json` exists with a different `extends`,
  change it to `@tsconfig/strictest/tsconfig.json`. If the
  old base provided options that `@tsconfig/strictest` does
  not, preserve them in `compilerOptions`.
- If `tsconfig.json` exists with no `extends`, add the
  field.
- If `tsconfig.json` does not exist, create a minimal one
  with just the `extends` field — leave project-specific
  options (`target`, `module`, `outDir`, `include`,
  `exclude`) to the user. These are structural choices,
  not strictness config.
- If individual strict options are explicitly set in
  `compilerOptions` that `@tsconfig/strictest` already
  covers, remove the redundant entries — they are now
  inherited.

## eslint.config.mjs

Create or update to use `typescript-eslint` with strict
type-checked linting:

```javascript
import tseslint from 'typescript-eslint';

export default tseslint.config(
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
);
```

### Merging

- If `eslint.config.mjs` exists and already includes both
  `strictTypeChecked` and `stylisticTypeChecked`, skip.
- If it exists with weaker configs (e.g. `recommended`),
  upgrade to `strictTypeChecked` + `stylisticTypeChecked`.
- If it exists with additional custom rules or overrides,
  preserve them — only ensure the base configs are strict.
- If no ESLint config exists, create the file above.

## package.json devDependencies

Ensure these devDependencies exist (add if missing, do not
downgrade if a newer version is already present):

| Package | Range | Purpose |
|---|---|---|
| `@tsconfig/strictest` | `^2.0.0` | Strictest tsconfig base |
| `typescript` | `^5.7.0` | TypeScript compiler |
| `typescript-eslint` | `^8.0.0` | Type-aware ESLint |
| `eslint` | `^9.0.0` | Linter |

### Merging

- If a dependency already exists with an equal or newer
  range, leave it.
- If absent, add it.
- Do not touch `dependencies` — these are all dev tools.
