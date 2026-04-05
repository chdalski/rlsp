# rlsp-yaml-parser

A spec-faithful YAML 1.2 parser for the [rlsp](https://github.com/chdalski/rlsp) language server project.

## Overview

`rlsp-yaml-parser` implements the full YAML 1.2 grammar by transliterating each of the 211 formal productions from the specification into a parser combinator function. Comments and spans are first-class data, making it suitable for editor tooling, linters, and formatters where precise source locations matter.

## Features

- **Spec-faithful** -- every production from the [YAML 1.2 specification](https://yaml.org/spec/1.2.2/) is implemented directly
- **100% conformance** -- passes 351/351 cases in the [YAML Test Suite](https://github.com/yaml/yaml-test-suite)
- **First-class comments** -- comments are preserved in the event stream and AST, not discarded
- **Lossless spans** -- every token, event, and AST node carries a `Span` with byte offsets back to the source
- **Alias preservation** -- lossless mode keeps alias references as `Node::Alias` nodes instead of expanding them
- **Security controls** -- alias expansion limits, nesting depth caps, anchor count limits, and cycle detection protect against untrusted input

## Quick Start

### Parse events

Stream through low-level parse events without building an AST:

```rust
use rlsp_yaml_parser::parse_events;
use rlsp_yaml_parser::event::Event;

for result in parse_events("hello: world\n") {
    let (event, span) = result.unwrap();
    println!("{event:?} at {span:?}");
}
```

### Load documents

Parse into an AST:

```rust
use rlsp_yaml_parser::load;

let docs = load("hello: world\n").unwrap();
assert_eq!(docs.len(), 1);
```

Use `LoaderBuilder` for fine-grained control:

```rust
use rlsp_yaml_parser::loader::LoaderBuilder;

let docs = LoaderBuilder::new()
    .resolved()              // expand aliases inline
    .max_nesting_depth(128)  // tighten nesting limit
    .build()
    .load("items:\n  - one\n  - two\n")
    .unwrap();
```

### Emit YAML

Convert an AST back to YAML text:

```rust
use rlsp_yaml_parser::load;
use rlsp_yaml_parser::emitter::{emit, EmitConfig};

let docs = load("hello: world\n").unwrap();
let output = emit(&docs, &EmitConfig::default());
println!("{output}");
```

## API Overview

| Module | Entry point | Purpose |
|--------|-------------|---------|
| `stream` | `tokenize(input)` | Tokenize YAML into a flat token list |
| `event` | `parse_events(input)` | Stream parse events with spans |
| `loader` | `load(input)` / `LoaderBuilder` | Build an AST (`Vec<Document<Span>>`) |
| `emitter` | `emit(docs, config)` | Emit AST back to YAML text |
| `schema` | `CoreSchema` / `JsonSchema` / `FailsafeSchema` | Resolve scalars to typed values |
| `node` | `Document`, `Node` | AST types with anchor, tag, and span data |

### Schemas

Three built-in schemas resolve untagged scalars to typed values:

| Schema | Behaviour |
|--------|-----------|
| `CoreSchema` | YAML 1.2 Core -- null, bool, int (decimal/octal/hex), float |
| `JsonSchema` | Strict JSON-compatible type inference |
| `FailsafeSchema` | All scalars are strings |

The `Schema` trait is object-safe for custom implementations.

### Security Limits

The loader enforces configurable limits to protect against malicious input:

| Limit | Default | Purpose |
|-------|---------|---------|
| `max_nesting_depth` | 512 | Prevents stack exhaustion from deeply nested structures |
| `max_anchors` | 10,000 | Bounds anchor-map memory |
| `max_expanded_nodes` | 1,000,000 | Guards against alias bombs (resolved mode only) |

Circular alias references are detected and reported as errors in both modes.

## Conformance

351/351 test cases pass from the [YAML Test Suite](https://github.com/yaml/yaml-test-suite) (valid and invalid inputs).

```sh
cargo test -p rlsp-yaml-parser --test conformance
```

## Performance

Criterion benchmarks compare `rlsp-yaml-parser` against [libfyaml](https://github.com/pantoniou/libfyaml) (a C reference parser). The table below shows representative throughput on synthetic fixtures (higher is better):

| Fixture | rlsp-yaml-parser (`load`) | libfyaml (`parse_events`) |
|---------|--------------------------|--------------------------|
| 100 B (tiny) | ~0.7 MB/s | ~33 MB/s |
| 10 KB (medium) | ~0.6 MB/s | ~100 MB/s |
| 100 KB (large) | ~0.5 MB/s | ~115 MB/s |

libfyaml is a highly optimized C library. `rlsp-yaml-parser` prioritizes correctness and spec fidelity over raw speed -- it tokenizes eagerly to provide full span coverage. Performance is sufficient for interactive editor use (the LSP use case) where documents are typically small.

Three benchmark suites are included:

- **Throughput** (`throughput`) -- MB/s across document sizes and YAML styles
- **Latency** (`latency`) -- time-to-first-event for streaming scenarios
- **Memory** (`memory`) -- allocation count and bytes during parse

```sh
cargo bench -p rlsp-yaml-parser
```

## Building

```sh
cargo build -p rlsp-yaml-parser
cargo test -p rlsp-yaml-parser
cargo clippy -p rlsp-yaml-parser  # pedantic + nursery, zero warnings
cargo bench -p rlsp-yaml-parser   # Criterion benchmarks
```

## License

[MIT](../LICENSE) -- Christoph Dalski
