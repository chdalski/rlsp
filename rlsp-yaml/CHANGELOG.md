# Changelog


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
