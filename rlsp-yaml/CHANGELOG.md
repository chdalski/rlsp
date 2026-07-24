# Changelog


## [0.13.1] - 2026-07-24

### Bug Fixes

- Patch brace-expansion and fast-uri npm advisories (5cd73a0)

### Documentation

- Document plugin and add marketplace catalog (e7300b8)
- Fix data-dir id in Task 2 verification steps (bc11ced)
- Document post-install session restart in README (7d836a2)

### Features

- Add plugin skeleton and PATH-binary LSP wiring (ba828c3)
- Auto-provision rlsp-yaml via SessionStart hook (653b91c)

### Refactoring

- Convert plugin to bring-your-own-binary (a7a937c)

## [0.13.0] - 2026-06-08

### Bug Fixes

- Omit unset formatPrintWidth from LSP settings (a558adf)
- Patch tmp/qs/uuid Dependabot alerts in lockfile (b388a06)

### Documentation

- Document formatEnable setting and external formatter interop (abb664f)

### Features

- Add formatEnable setting to gate LSP formatting handlers (b61430c)
- Add .editorconfig parser module and cache (666b902)
- Integrate .editorconfig into format handlers and watcher (1478a35)
- Add formatRespectEditorconfig opt-out and docs (032e5b3)

### Refactoring

- Rename lsp_lifecycle.rs to lsp_lifecycle/main.rs (570fb7f)
- Extract shared LSP test helpers into helpers.rs (c4eadb9)
- Extract per-capability LSP test modules (3465ae3)
- Extract configuration and schema-routing test modules (07511a9)
- Extract validators_integration test module (d9f8a56)
- Extract formatting_integration test module (3ab2cdd)
- Rename corpus_invariants.rs to corpus_invariants/main.rs (d002edd)
- Extract corpus_invariants shared utilities (9ff51d1)
- Extract I1-I4 invariant test modules (13934e5)
- Extract I5, I6, I8, I9 invariant test modules (c605197)
- Extract I10 and I11 invariant test modules (3e943e8)
- Move residual unit tests from corpus_invariants main.rs to shared (1025fa4)
- Extract custom_tag and anchors validator submodules (37ac44b)
- Extract flow_style and key_ordering submodules (b7ad482)
- Extract duplicate_keys and yaml11_compat validator submodules (2fc45f2)
- Extract custom_tags_validation submodule (9b28abd)
- Extract options and scalar_render formatter submodules (db2b7c8)
- Extract dedup formatter submodule (ace5ac8)
- Extract sequence_render and mapping_render formatter submodules (683d9f3)
- Extract comment_preservation and content_tracking formatter submodules (a9c684c)
- Extract node_to_doc formatter submodule (4e08660)
- Extract context and support schema_validation submodules (c37dda0)
- Extract type_validation schema_validation submodule (8e942c3)
- Extract composition schema_validation submodule (ae453bd)
- Extract array_constraints schema_validation submodule (9afc2b2)
- Extract scalar_constraints schema_validation submodule (f6bb764)
- Extract mapping_constraints submodule from schema_validation (da28b7c)
- Finish dispatcher-only schema_validation parent (d777108)
- Strip stale numeric test markers (c194494)
- Extract formatting and support submodules from completion.rs (d2077de)
- Extract cursor_location submodule from completion.rs (6777776)
- Extract navigation submodule from completion.rs (803905d)
- Extract completion_items and completion_drivers submodules (39e8a06)
- Extract schema_completions submodule from completion.rs (d64ee4a)

## [0.12.1] - 2026-05-13

### Bug Fixes

- Pin patched fast-uri and serialize-javascript via pnpm overrides (b909efd)

### Documentation

- Document Zed extension across project docs (57171ee)
- Document Zed multi-LSP coexistence (4e0c30e)

## [0.12.0] - 2026-05-13

### Bug Fixes

- Strengthen idempotent fixture assertion to format(input) == input (f1830cf)
- Add bracket spacing to code-action flow mapping fixtures (b85330c)

### Features

- Mark error types non_exhaustive (90e5b7c)
- Map InvalidCharacter errors to distinct LSP diagnostic code (ce9f439)
- Block-to-flow handles nested collections (5dfb268)
- Extend string_to_block_scalar to sequence items (af42644)
- Add "Convert to block scalar (folded)" code action (b65a661)
- Suppress block-to-flow action under enforce-block policy (8e12e69)
- Add detail text and label-key heuristic to document symbols (6258d06)
- Support non-mapping roots and multi-doc wrappers in document symbols (c2a631e)
- Add custom tag type annotations with tagTypeMismatch diagnostic (19790bb)

## [0.11.0] - 2026-05-05

### Bug Fixes

- Clear properties from cloned scalar in string_to_block_scalar (6e3abf3)
- Preserve node properties across cursor-driven code actions (daf3d21)
- Add position fields to LoadError variants (4f4a840)

### Features

- Plumb YamlFormatOptions through code_actions dispatch (79a95fc)

### Refactoring

- Drop block_to_flow's hardcoded long-line warning (2e7f088)
- Extract shared integration-test helpers to tests/common (3098cd3)
- Resolve flow-style severity at the validator (cd95297)
- Resolve duplicate-key severity at the validator (bca67ff)

## [0.10.1] - 2026-04-27

### Bug Fixes

- Revert manual version bumps — release-plz manages versions (b96bedc)

### Performance

- Borrow constant tag URIs from resolver (3f15780)
- Box rare per-node metadata behind Option<Box<NodeMeta>> (d853605)
- Replace Span Pos pair with u32 byte offsets, add LineIndex (716771f)

## Bug Fixes

- Suppress resolver-injected tags on empty scalars (34bcc58)
- Drop use_tabs formatter option that violates YAML 1.2 §6.1 (6f5e075)

## Documentation

- Remove internal-refactor entries from feature-log (d00d48a)
- Add formatIndentSequences to configuration and feature log (90c4b1c)
- Move tab_to_spaces retrofit note to module doc comment (9c21f9b)

## Features

- Add JSON and Failsafe schema resolution variants (60c095c)
- Add formatIndentSequences setting (7f1b161)
- Add formatIndentSequences setting (fb84f02)

## Refactoring

- Migrate type-inference callsites to tag-URI comparisons (8ecdeb5)
- Remove dead type-classification functions from scalar_helpers (e8139ef)
- Hoist module-scope use/mod to header per import-placement rule (698263d)
- Hoist fn-body use statements per import-placement rule (f984301)
- Reorder sub-module use/mod headers per import-placement rule (543cd93)

## Bug Fixes

- Add complete_at corpus invariant and cap structural completions (aef054d)

## Documentation

- Document formatPreserveQuotes option (e2ad316)
- Add corpus-invariants WORKLIST and feature-log entry (1b9ffbb)
- Add feature-log entry for string_to_block_scalar AST retrofit (52b2002)
- Clarify feature-log.md is user-facing only (1feb3df)
- Move feature-log agent guidance to hidden HTML comment (9860187)

## Features

- Add preserve_quotes formatter option plumbing (b844105)
- Honor preserveQuotes in scalar emission (5a6677a)
- Add format_subtree public API to formatter (8dfe0e0)
- Rewrite flow-to-block code actions via AST + format_subtree (957c80f)
- Rewrite block_to_flow via AST + format_subtree (173f838)
- Expose anchor_loc span on AST nodes (0ca3083)
- Propagate tag_loc through AST nodes (a7870c9)
- Add AST cursor-context substrate for complete_at (1f93709)

## Performance

- Cache boundary-audit regexes with LazyLock (4b81a87)

## Refactoring

- Retrofit validate_flow_style to consume AST (9c5a6e1)
- Retire quote_flow_item and cover block_to_flow defect classes (b752319)
- Retrofit string_to_block_scalar to AST + format_subtree (370b8c4)
- Retrofit quoted_bool_to_unquoted to AST + format_subtree (c423caa)
- Retrofit yaml11_bool_actions and schema_yaml11_bool_type_actions to AST + format_subtree (5a7d793)
- Retrofit yaml11_octal_actions to AST + format_subtree (a7ab9ff)
- Retrofit delete_unused_anchor to AST + format_subtree (e2fde09)
- Retrofit validate_unused_anchors to AST-only (0848168)
- Retrofit custom-tags and key-ordering validators to AST (2733893)
- Retrofit validate_schema to AST-only position lookup (eab7e32)
- Retrofit hover_at to AST span-containment walk (d3a0d79)
- Retrofit navigation/references.rs to AST walk (947d07b)
- Retrofit rename to AST span-containment walk (1f607c5)
- Retrofit document_symbols to AST-only (5b85651)
- Retrofit selection_ranges to AST-only (c7094eb)
- Retrofit find_document_links to AST-only walk (90eecea)
- Retrofit find_colors to AST-only walk (273628c)
- Retrofit format_on_type to AST-only indentation (56c2771)
- Retrofit folding_ranges to AST + Event::Comment (82d76da)
- Retrofit semantic_tokens to AST + Event::Comment (4f71961)
- Retrofit complete_at to AST-first cursor location (011b79e)
- Consolidate test helpers into shared test_utils module (00a8e38)
- Split code_actions.rs into per-action submodules (99da38b)
- Parameterize repetitive schema_validation tests with rstest (6820481)
- Parameterize schema URL validation tests (8431d9a)

## Bug Fixes

- Rewrite while_let_loop sites and remove trailing commas (60202c2)

## Performance

- Wrap leading_comments in Option<Vec<String>> (e812232)

## Bug Fixes

- Indent block scalar content lines in formatter (0b31477)
- Preserve anchor definitions in formatter output (5309f84)
- Restrict single_quote option to values only (7390155)
- Preserve custom tags on mapping and sequence nodes (c45d048)
- Preserve leading comments in nested block mappings (168c0e3)
- Move fixture CLAUDE.md out of formatter glob path (a0e93aa)
- Preserve double-quoting for scalars with control characters (987aa89)
- Preserve quoting for whitespace-bounded and quote-starting scalars (25b1130)
- Fix anchor and tag placement on block collections (04d3fc0)
- Use key_needs_space_before_colon in flow mappings (WZ62) (c2ea92d)
- Handle multiline plain scalars and whitespace-only block lines (fe9fe80)
- Fall back to quoted for spaces-only block scalars (67d3087)
- Broaden block scalar whitespace guard for mixed space+tab lines (7df1712)
- Handle tags and quoted scalars inline after document start marker (620bdd9)
- Fix explicit key and empty key loader bugs (a8dff3b)
- Fix anchor/alias loader bugs for conformance cases (9552340)
- Eliminate all loader conformance failures (d4b6602)
- Remove formatter workaround and fix all remaining conformance failures (96c0a57)

## Documentation

- Add setting-interaction coverage guidance for fixtures (41bde3c)
- Add idempotency-only fixture convention (3b62fb8)

## Features

- Add CollectionStyle to AST mapping and sequence nodes (728d182)
- Add flow-style rendering to the formatter (20004bb)
- Add flowStyle severity and formatEnforceBlockStyle settings (73d38db)
- Expose flow-style settings in VS Code extension and docs (875e216)
- Add configurable duplicateKeys severity setting (efb3ab4)
- Add formatRemoveDuplicateKeys setting and dedup pre-pass (b6f52b1)
- Expose duplicate key settings in VS Code extension (4a24e16)
- Add block scalar indentation indicator and folded blank-line preservation (826d008)
- Add explicit key syntax support to the formatter (459f43a)
- Surface document marker flags in AST (4740d10)
- Add interacting-settings fixtures, wire bracket_spacing, and update docs (1c2e974)

## Bug Fixes

- Update dev dependencies to resolve 3 high/medium Dependabot alerts (40d4678)
- Align engines.vscode with @types/vscode ^1.115.0 (4032830)
- Update major-version dev dependencies (dcaecc7)

## Documentation

- Document YAML 1.1 compatibility diagnostics (b7357cc)

## Features

- Declare workspace extension kind and untrusted workspace capabilities (f4a5786)
- Add YAML 1.1 boolean/octal detection and compatibility diagnostics (a7a47a6)
- Add quick fixes for YAML 1.1 booleans and octals (2a0130d)
- Add schema-aware severity escalation for YAML 1.1 values (ac0716d)
- Expose yamlVersion and validate settings (15b92fa)

## Refactoring

- Parameterize schema.rs modeline extraction tests with rstest (d608e6f)
- Parameterize schema_validation.rs tests with rstest (f969e21)
- Parameterize validators.rs tests with rstest (f3b8444)
- Parameterize formatter.rs tests with rstest (77b2033)
- Parameterize completion.rs + hover.rs tests with rstest (6a918bc)
- Parameterize document_links, rename, references tests with rstest (deffa03)
- Parameterize code_actions, semantic_tokens, symbols tests with rstest (71c5c2f)
- Parameterize remaining small test modules with rstest (930cf21)
- Extract format validators into schema_validation/formats.rs (dcdd239)
- Extract association functions from schema.rs into schema/association.rs (f45b206)
- Promote clippy::panic to workspace-level deny (37e66c0)
- Replace #[allow] with #[expect(reason)] and enforce via workspace lints (b248fca)
- Extract PlainScalarKind enum and classify_plain_scalar (6569e1c)
- Group flat source files into domain-based modules (52f0ce1)
- Convert schema modules from mod.rs to named files (722405d)

## Bug Fixes

- Add binary existence check and skip test when binary present (bb1179b)
- Decode quoted block mapping keys and attach trailing comments (620720f)

## Documentation

- Retrofit AI Note across all crate READMEs and list all crates in root (0bf9706)

## Features

- Replace PEG parser with streaming implementation (cc5c9a5)

## Bug Fixes

- Upgrade lodash to fix CVE-2026-4800 and CVE-2026-2950 (1f0cb2a)
- Fix false duplicate-key diagnostics for sibling mappings (e5e5cd8)
- Suppress flow-style warnings for empty collections (f34a305)
- Preserve blank lines between mapping entries (aeed0a5)
- Strip unnecessary quotes from Representation variants (e8e5e6b)
- Fix code action conversion bugs (3737c50)
- Prevent double-quoting of already-quoted items in block-to-flow conversion (44514c1)
- Use full container spans and simplify selection range logic (4694575)
- Restore alias key duplicate detection and add edge case tests (74e2fed)

## Documentation

- Document yamlVersion setting and modeline (9fdd6fb)
- Document diagnostic suppression comments (470cc12)
- Document feature toggles and maxItemsComputed settings (199f589)

## Features

- Add integration test infrastructure (b4c3f17)
- Add integration test suite (f9fa0cc)
- Switch formatter to early_parse(false) for style preservation (022d9d2)
- Add yamlVersion setting and modeline support (67e1401)
- Version-aware quoting in formatter (b514adc)
- Add diagnostic suppression comment parser (63daa76)
- Integrate diagnostic suppression with pipeline (26fb7c7)
- Add validate, hover, and completion feature toggles (9ce8e80)
- Add maxItemsComputed setting for symbols and folding (2c45fa0)
- Add rlsp-yaml-parser dependency and migrate entry points (a4af8bc)
- Migrate symbols, hover, and completion to rlsp-yaml-parser (8b0d3d9)
- Migrate validators, schema, and schema_validation to rlsp-yaml-parser (906ce2e)
- Migrate selection.rs and formatter.rs to rlsp-yaml-parser (6dce350)
- Wire up contentSchema validation (a26559b)
- Replace raw-text comment workaround with AST-based comments (c640283)

## Refactoring

- Use fmt::Write instead of push_str+format in tests (df9b991)
- Replace text-scanning duplicate key detection with AST-based approach (600fbcf)

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
