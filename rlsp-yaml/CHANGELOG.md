# Changelog


## Bug Fixes

- Standardize diagnostic message format (44ceffe)
- Standardize validator, parser, and schema error messages (f262004)
- Use platform-aware paths in server test assertions (774d849)

## Documentation

- Update plan progress — Task 5 complete (37500fa)
- Add VS Code extension references to project documentation (357ac25)
- Add extension CLAUDE.md with project conventions (a323470)
- Mark plan complete, generalize CLAUDE.md pnpm guidance (470028b)

## Features

- Wire remote $ref resolution into server (632f66e)

## Refactoring

- Move extension to rlsp-yaml/editors/code/ (db8ae1a)
- Rename editors/code to integrations/vscode (ed25b7c)
- Remove dead code from schema.rs (16a071a)

## Features

- Add Criterion benchmark infrastructure (78a03cd)
- Add Tier 1 hot-path benchmarks (79e74e6)
- Add Tier 2 user-perceivable latency benchmarks (279b156)
- Add Tier 3 architectural insight benchmarks (e0170ac)

## Refactoring

- Replace linear scans with pre-built key index (15e783d)

## Documentation

- Remove hardcoded test count from readme (cfb6fbb)

## Refactoring

- Replace expect/unwrap with safe alternatives and allow test lints (7092dcd)
- Fix 88 clippy violations in test code (ac8b43f)

## Documentation

- Restructure documentation into three-tier layout (ee94187)

## Bug Fixes

- Fix cargo fmt line-wrapping in content tests (2f8ebb4)

## Documentation

- Add plan for schema resolution and format validation (db14873)
- Mark schema resolution and format plan complete (db671a9)
- Add Helix and Zed editor setup to configuration (c0ddf40)
- Document minProperties/maxProperties and additionalItems in feature log (2c09807)

## Features

- Detect JSON Schema draft from $schema URI (e82de4b)
- Resolve $id / id base URI and thread through sub-schema parsing (f36c93b)
- Thread ParseContext through all sub-schema parsing for remote $ref (3d10310)
- Validate format keyword with 15 format validators (05dbcd7)
- Add IDN/IRI format validators (075e74d)
- Validate contentEncoding and contentMediaType keywords (9716140)
- Default Kubernetes schema version to master (c0b7367)
- Add minProperties/maxProperties object validation (1963542)
- Add additionalItems validation for Draft-04/07 tuple arrays (6bcfef4)

## Bug Fixes

- Harden regex compilation against ReDoS (13d62b5)

## Documentation

- Reformat feature-log and add schema/color gaps (38d8624)
- Mark all JSON Schema keywords as completed (f86a742)

## Features

- Add scalar constraint validation (42d392b)
- Add not keyword validation (baeeef8)
- Add patternProperties validation (dca6cdc)
- Add array constraint validation (24ed3c4)
- Add propertyNames validation (5712efc)
- Add dependencies/dependentRequired/dependentSchemas (352eec8)
- Add if/then/else conditional validation (6fbecf1)
- Add contains/minContains/maxContains validation (2f0e3ee)
- Add prefixItems / Draft-04 tuple validation (d5a20e8)
- Add $anchor/$dynamicRef/$dynamicAnchor resolution (693cd37)
- Add unevaluatedProperties/unevaluatedItems (a1427b7)
- Add $vocabulary parsing and check_vocabulary (5c95a0f)
- Add color provider for YAML value color detection (7c91ff9)

## Refactoring

- Simplify fits() by removing dead break-mode branches (5219f94)

## Bug Fixes

- Add connect and global timeouts to HTTP agent (968462b)
- Add rename length cap and close IPv6 SSRF gaps (c658003)
- Add README and version dep for crates.io publishing (c3d0429)

## Documentation

- Document kubernetesVersion setting and mark K8s feature complete (630e0d9)
- Document schemaStore setting and mark feature complete (6450d8e)
- Document formatting feature and settings (8663af8)
- Document range formatting support (3602630)
- Document httpProxy setting and mark proxy support complete (c1da0f7)
- Add HTTP timeout limits to Schema Fetching section (bcac04a)

## Features

- Add Kubernetes resource detection and schema URL construction (77c4298)
- Wire Kubernetes auto-detection into parse_and_publish (9232a96)
- Add SchemaStore catalog fetch, parse, and matching (34d135a)
- Integrate SchemaStore catalog as fourth schema fallback (128583f)
- Add YAML formatter AST walker (1e14890)
- Preserve YAML comments through formatting (2afd833)
- Add LSP formatting handler and settings (874ffa5)
- Add document range formatting support (579b0ca)
- Add HTTP proxy support for schema fetching (cb99e67)

## Bug Fixes

- Add connect and global timeouts to HTTP agent (968462b)
- Add rename length cap and close IPv6 SSRF gaps (c658003)

## Documentation

- Document kubernetesVersion setting and mark K8s feature complete (630e0d9)
- Document schemaStore setting and mark feature complete (6450d8e)
- Document formatting feature and settings (8663af8)
- Document range formatting support (3602630)
- Document httpProxy setting and mark proxy support complete (c1da0f7)
- Add HTTP timeout limits to Schema Fetching section (bcac04a)

## Features

- Add Kubernetes resource detection and schema URL construction (77c4298)
- Wire Kubernetes auto-detection into parse_and_publish (9232a96)
- Add SchemaStore catalog fetch, parse, and matching (34d135a)
- Integrate SchemaStore catalog as fourth schema fallback (128583f)
- Add YAML formatter AST walker (1e14890)
- Preserve YAML comments through formatting (2afd833)
- Add LSP formatting handler and settings (874ffa5)
- Add document range formatting support (579b0ca)
- Add HTTP proxy support for schema fetching (cb99e67)

## Bug Fixes

- Add connect and global timeouts to HTTP agent (968462b)
- Add rename length cap and close IPv6 SSRF gaps (c658003)

## Documentation

- Document kubernetesVersion setting and mark K8s feature complete (630e0d9)
- Document schemaStore setting and mark feature complete (6450d8e)
- Document formatting feature and settings (8663af8)
- Document range formatting support (3602630)
- Document httpProxy setting and mark proxy support complete (c1da0f7)
- Add HTTP timeout limits to Schema Fetching section (bcac04a)

## Features

- Add Kubernetes resource detection and schema URL construction (77c4298)
- Wire Kubernetes auto-detection into parse_and_publish (9232a96)
- Add SchemaStore catalog fetch, parse, and matching (34d135a)
- Integrate SchemaStore catalog as fourth schema fallback (128583f)
- Add YAML formatter AST walker (1e14890)
- Preserve YAML comments through formatting (2afd833)
- Add LSP formatting handler and settings (874ffa5)
- Add document range formatting support (579b0ca)
- Add HTTP proxy support for schema fetching (cb99e67)

## Bug Fixes

- Correct GitHub username cdalski → chdalski in all URLs (b91abd7)
