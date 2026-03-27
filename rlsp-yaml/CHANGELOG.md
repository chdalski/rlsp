# Changelog


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
