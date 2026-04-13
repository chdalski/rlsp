# rlsp-yaml-parser

Spec-faithful streaming YAML 1.2 parser.

## Overview

`rlsp-yaml-parser` parses YAML text using a streaming state-machine
architecture. A line-oriented lexer splits input into lines once and hands
them to an event iterator that walks the state machine without backtracking.
Each call to the iterator's `next()` produces at most one event, giving O(1)
first-event latency regardless of input size. A separate loader consumes the
event stream and builds an AST when a tree representation is needed.

## Features

- **Spec-faithful** — tested against the [YAML Test Suite](https://github.com/yaml/yaml-test-suite); passes 368/368 test cases
- **Streaming** — zero-copy event iterator; does not materialise the full AST unless you call the loader
- **First-class comments** — comment text and spans are preserved and attached to adjacent AST nodes by the loader
- **Lossless spans** — every event and AST node carries a `Span` covering the exact input bytes that produced it
- **Alias preservation** — `LoadMode::Lossless` (default) keeps alias references as `Node::Alias` nodes; `LoadMode::Resolved` expands them inline
- **Security controls** — configurable nesting depth, anchor count, and alias-expansion node limits guard against denial-of-service inputs

## Conformance

Tested against the [YAML Test Suite](https://github.com/yaml/yaml-test-suite):

```
368 / 368 test cases pass
```

Run it yourself:

```sh
cargo test -p rlsp-yaml-parser --test conformance
```

## Quick Start

### Parse events directly

```rust
use rlsp_yaml_parser::{parse_events, Event};

for result in parse_events("key: value\n") {
    let (event, span) = result.unwrap();
    println!("{event:?} @ {span:?}");
}
```

### Load into an AST (convenience entry point)

```rust
use rlsp_yaml_parser::loader::load;

let docs = load("key: value\n").unwrap();
println!("{:?}", docs[0].root);
```

### Load with custom options

```rust
use rlsp_yaml_parser::loader::LoaderBuilder;

let docs = LoaderBuilder::new()
    .resolved()
    .max_nesting_depth(128)
    .build()
    .load("key: value\n")
    .unwrap();
```

## API Overview

| Item | Description |
|------|-------------|
| `parse_events(input)` | Returns a lazy `Iterator<Item = Result<(Event, Span), Error>>` |
| `loader` | `load`, `Loader`, `LoaderBuilder`, `LoaderOptions`, `LoadMode`, `LoadError` |
| `node` | `Document`, `Node` — AST types produced by the loader |
| `event` | `Event`, `ScalarStyle`, `Chomp`, `CollectionStyle` — event types |
| `encoding` | UTF-8/16/32 and BOM detection; typically internal use |
| `lines` | `Line`, `LineBuffer`, `BreakType` — line-oriented lexer primitives; typically internal use |

## Security Limits

The loader enforces three configurable limits to guard against
denial-of-service inputs. All limits are active in both lossless and
resolved modes unless noted.

| Option | Default | Guards against |
|--------|---------|----------------|
| `max_nesting_depth` | 512 | Stack exhaustion from deeply nested collections |
| `max_anchors` | 10 000 | Unbounded anchor-map memory growth |
| `max_expanded_nodes` | 1 000 000 | Alias bombs (Billion Laughs); resolved mode only |

Override defaults via `LoaderBuilder` or by constructing `LoaderOptions`
directly and passing it to `Loader`.

## Performance

The streaming architecture delivers sub-microsecond first-event latency on
realistic YAML inputs, competitive with libfyaml.
[See docs/benchmarks.md](docs/benchmarks.md) for detailed measurements.

## Documentation

- [Architecture](docs/architecture.md) — streaming state-machine design, O(1) latency, comment attachment, security limits
- [Feature Log](docs/feature-log.md) — implemented capabilities and design decisions
- [Benchmarks](docs/benchmarks.md) — performance measurements and methodology

## Building

```sh
cargo build  -p rlsp-yaml-parser
cargo test   -p rlsp-yaml-parser
cargo clippy -p rlsp-yaml-parser --all-targets
cargo bench  -p rlsp-yaml-parser
```

## License

[MIT](../LICENSE) — Christoph Dalski

## AI Note

Every line of source in this crate was authored, reviewed, and committed by AI agents
working through a multi-agent pipeline (planning, implementation, independent review,
and test/security advisors for high-risk tasks). The human role is designing the
architecture, rules, and review process; agents execute them. Conformance against the
YAML Test Suite is a measured acceptance criterion — not an aspiration — and any change
touching parser behaviour or untrusted input passes through formal test and security
advisor review before being merged.
