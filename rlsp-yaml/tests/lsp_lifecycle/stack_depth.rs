// SPDX-License-Identifier: MIT
//
// Pins that the whole server holds both ends of the `maxNestingDepth` setting:
// a stack overflow in any handler ends the process, and tower-lsp polls the
// handlers on the thread that runs `Server::serve`, so every in-process test
// here runs the service on a thread of a fixed stack size, the way `main` does.
//
// `RLSP_STACK_KIB` replaces the stack size of the thread, which is how the
// smallest size that passes is measured: one process per probe, since glibc
// caches thread stacks and a thread may get a larger one than it asked for
// after an earlier thread in the same process.
use std::future::Future;
use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rlsp_yaml::server::Backend;
use rstest::rstest;
use serde_json::{Value, json};
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::{Diagnostic, NumberOrString};

use super::code_actions::code_action_request_with_diagnostics;
use super::code_lens::code_lens_request;
use super::completion::completion_request;
use super::configuration::did_change_configuration_notification;
use super::document_color::document_color_request;
use super::document_links::document_link_request;
use super::fake_proxy::{FakeProxy, Reply, SCHEMA_HOST};
use super::folding_ranges::folding_range_request;
use super::formatting_integration::{
    formatting_request_with_tab_size, range_formatting_request_with_tab_size,
};
use super::helpers::*;
use super::hover::hover_request;
use super::navigation::{definition_request, document_symbol_request, references_request};
use super::nesting_depth::{diagnostics_of, has_symbols, nested};
use super::on_type_formatting::on_type_formatting_request;
use super::rename::{prepare_rename_request, rename_request};
use super::selection_ranges::selection_range_request;
use super::semantic_tokens::semantic_tokens_request;
use super::watched_files::did_change_watched_files_notification;

const SCHEMA_URL: &str = "http://schema.invalid/s.json";
const URI: &str = "file:///test/deep.yaml";

/// The default limit.
const DEFAULT_LEVELS: usize = 256;
/// The stack of a tokio worker thread, which the debug runs use.
const TWO_MIB: usize = 2 * 1024 * 1024;

/// The upper end of the setting.
const MAX_LEVELS: usize = 512;
/// 75% of `SERVER_STACK_SIZE` in `src/main.rs`, which the release runs use.
const RELEASE_STACK: usize = 6 * 1024 * 1024;

/// Run `body` the way `main` runs the server: on a thread of `stack_bytes`
/// that polls the future itself, with no task spawned around it.
///
/// `thread_stack_size` gives the `spawn_blocking` threads, where the schema is
/// fetched and parsed, the same stack. The thread is started by the test
/// itself, before any other thread of the test.
fn run_on_stack<F, Fut>(name: String, stack_bytes: usize, body: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()>,
{
    let stack_bytes = std::env::var("RLSP_STACK_KIB").map_or(stack_bytes, |kib| {
        1024 * kib
            .parse::<usize>()
            .expect("RLSP_STACK_KIB is a number of KiB")
    });
    std::thread::Builder::new()
        .name(name)
        .stack_size(stack_bytes)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .thread_stack_size(stack_bytes)
                .build()
                .expect("tokio runtime")
                // The handlers' futures live on the heap in tower-lsp; boxing
                // keeps the test's own future off the measured stack as well.
                .block_on(Box::pin(body()));
        })
        .expect("spawn the stack thread")
        .join()
        .expect("the stack thread panicked");
}

fn lsp(number: usize) -> u32 {
    u32::try_from(number).expect("a position fits in u32")
}

/// A document shape of exactly `levels` nested collections. Each shape is the
/// tightest for some handler, see the comments on the variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    BlockMappings,
    BlockSequences,
    FlowMappings,
    FlowSequences,
    /// Complex keys nested in complex keys: the formatter's tightest shape.
    QuestionKeys,
    /// Block mappings with an anchor on the first line and an alias, a URL and
    /// a color in the deepest mapping, so the navigation and decorator
    /// handlers have something to find at the bottom.
    AnchorAndAlias,
}

/// The requests that read the stored AST or call the formatter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    DocumentSymbol,
    SelectionRange,
    FoldingRange,
    SemanticTokens,
    Hover,
    Completion,
    Definition,
    References,
    PrepareRename,
    Rename,
    DocumentLink,
    CodeLens,
    DocumentColor,
    Formatting,
    RangeFormatting,
    OnTypeFormatting,
    CodeAction,
}

/// The formatter options in force.
#[derive(Clone, Copy, Debug)]
enum Formatter {
    Default,
    /// Converts flow collections to block style and removes duplicate keys.
    EnforceBlockStyle,
}

impl Shape {
    fn text(self, levels: usize) -> String {
        let indent = |level: usize| "  ".repeat(level);
        match self {
            Self::BlockMappings => nested(levels),
            Self::BlockSequences => (0..levels)
                .map(|level| {
                    let item = if level + 1 == levels { "-   x" } else { "-" };
                    [indent(level).as_str(), item, "\n"].concat()
                })
                .collect(),
            Self::QuestionKeys => (0..levels)
                .map(|level| {
                    let item = if level + 1 == levels { "-   x" } else { "?" };
                    [indent(level).as_str(), item, "\n"].concat()
                })
                .collect(),
            Self::FlowMappings => format!("{}1{}\n", "{a: ".repeat(levels), "}".repeat(levels)),
            Self::FlowSequences => format!("{}1{}\n", "[".repeat(levels), "]".repeat(levels)),
            Self::AnchorAndAlias => {
                let chain: String = (0..levels - 1)
                    .map(|level| [indent(level).as_str(), "k:\n"].concat())
                    .collect();
                let leaf = indent(levels - 1);
                format!(
                    "anchor: &t text\n{chain}{leaf}leaf:   *t\n{leaf}link: https://example.com/x\n\
                     {leaf}color: \"#ff0000\"\n"
                )
            }
        }
    }

    /// The position of the deepest node.
    fn deep(self, levels: usize) -> (u32, u32) {
        let last = levels - 1;
        match self {
            Self::BlockMappings => (lsp(last), lsp(2 * last + 1)),
            Self::BlockSequences | Self::QuestionKeys => (lsp(last), lsp(2 * last + 4)),
            Self::FlowMappings => (0, lsp(4 * last + 1)),
            Self::FlowSequences => (0, lsp(levels)),
            Self::AnchorAndAlias => (lsp(levels), lsp(2 * last + 8)),
        }
    }

    /// The position of the anchor, or the start of the document.
    const fn anchor(self) -> (u32, u32) {
        match self {
            Self::AnchorAndAlias => (0, 9),
            Self::BlockMappings
            | Self::BlockSequences
            | Self::FlowMappings
            | Self::FlowSequences
            | Self::QuestionKeys => (0, 0),
        }
    }

    /// The line of the outermost collection that `codeAction` can convert.
    const fn outermost_line(self) -> u32 {
        match self {
            Self::AnchorAndAlias => 1,
            Self::BlockMappings
            | Self::BlockSequences
            | Self::FlowMappings
            | Self::FlowSequences
            | Self::QuestionKeys => 0,
        }
    }

    fn last_line(self, levels: usize) -> u32 {
        match self {
            Self::FlowMappings | Self::FlowSequences => 1,
            Self::BlockMappings | Self::BlockSequences | Self::QuestionKeys => lsp(levels - 1),
            Self::AnchorAndAlias => lsp(levels + 2),
        }
    }

    /// The conversion `codeAction` offers at the start of the document, which
    /// formats the whole tree. Nothing converts to flow style while the
    /// formatter enforces block style.
    const fn conversion(self, formatter: Formatter) -> Option<&'static str> {
        match (self, formatter) {
            (Self::BlockMappings | Self::AnchorAndAlias, Formatter::Default) => {
                Some("block to flow")
            }
            (Self::FlowMappings | Self::FlowSequences, _) => Some("to block style"),
            _ => None,
        }
    }
}

/// Every request that reads the AST or calls the formatter, for a document of
/// `shape` and `levels`. `diagnostics` are the ones the server published for
/// it, which `codeAction` carries in its context.
fn requests(shape: Shape, levels: usize, diagnostics: &Value) -> Vec<(Call, Request)> {
    let (line, character) = shape.deep(levels);
    let (anchor_line, anchor_character) = shape.anchor();
    vec![
        (Call::DocumentSymbol, document_symbol_request(1, URI)),
        (
            Call::SelectionRange,
            selection_range_request(2, URI, line, character),
        ),
        (Call::FoldingRange, folding_range_request(3, URI)),
        (Call::SemanticTokens, semantic_tokens_request(4, URI)),
        (Call::Hover, hover_request(5, URI, line, character)),
        (
            Call::Completion,
            completion_request(6, URI, line, character),
        ),
        (
            Call::Definition,
            definition_request(7, URI, line, character),
        ),
        (
            Call::References,
            references_request(8, URI, anchor_line, anchor_character, true),
        ),
        (
            Call::PrepareRename,
            prepare_rename_request(9, URI, line, character),
        ),
        (Call::Rename, rename_request(10, URI, line, character, "zz")),
        (Call::DocumentLink, document_link_request(11, URI)),
        (Call::CodeLens, code_lens_request(12, URI)),
        (Call::DocumentColor, document_color_request(13, URI)),
        (
            Call::Formatting,
            formatting_request_with_tab_size(14, URI, 2),
        ),
        (
            Call::RangeFormatting,
            range_formatting_request_with_tab_size(15, URI, 0, shape.last_line(levels), 2),
        ),
        (
            Call::OnTypeFormatting,
            on_type_formatting_request(16, URI, line, character, "\n"),
        ),
        (
            Call::CodeAction,
            code_action_request_with_diagnostics(
                17,
                URI,
                shape.outermost_line(),
                shape.outermost_line() + 1,
                diagnostics,
            ),
        ),
    ]
}

fn count(json: &str, needle: &str) -> usize {
    json.matches(needle).count()
}

/// How deep into the document the answer reaches. Measured on the text of the
/// result, since a nested result is deeper than `serde_json` reads back.
fn reach(call: Call, json: &str) -> usize {
    match call {
        Call::DocumentSymbol => count(json, "\"children\":["),
        Call::SelectionRange => count(json, "\"parent\":"),
        Call::FoldingRange => count(json, "\"startLine\""),
        Call::SemanticTokens => count(json, ",") / 5,
        Call::Rename => count(json, "\"newText\""),
        Call::References => count(json, "\"range\""),
        // The edits are flat, so they read back; their text is a block
        // document of one line per level, or a flow document of one bracket
        // per level.
        Call::Formatting | Call::RangeFormatting => {
            let edits: Value = serde_json::from_str(json).expect("the edits are not nested");
            edits
                .as_array()
                .and_then(|edits| edits.first())
                .and_then(|edit| edit.get("newText"))
                .and_then(Value::as_str)
                .map_or(0, |text| {
                    text.lines().count().max(text.matches(['{', '[']).count())
                })
        }
        Call::Hover
        | Call::Completion
        | Call::Definition
        | Call::PrepareRename
        | Call::DocumentLink
        | Call::CodeLens
        | Call::DocumentColor
        | Call::OnTypeFormatting
        | Call::CodeAction => usize::from(json != "null" && json != "[]" && json != "{}"),
    }
}

/// How deep the answer to `call` must reach for a document of `shape` and
/// `levels`: the bottom of the document where the handler follows it, 1 where
/// it answers something at the bottom, and 0 where the shape gives the
/// handler nothing to find. A flow document is one line, which a range cannot
/// cut, so `rangeFormatting` claims nothing for it.
fn expected_reach(shape: Shape, call: Call, levels: usize) -> usize {
    let flow = matches!(shape, Shape::FlowMappings | Shape::FlowSequences);
    let anchored = matches!(shape, Shape::AnchorAndAlias);
    let has_keys = matches!(
        shape,
        Shape::BlockMappings | Shape::FlowMappings | Shape::AnchorAndAlias
    );
    let full_or_nothing = |reaches: bool| if reaches { levels } else { 0 };
    match call {
        Call::DocumentSymbol => full_or_nothing(has_keys).saturating_sub(1),
        Call::SelectionRange => full_or_nothing(!matches!(shape, Shape::QuestionKeys)),
        Call::FoldingRange | Call::RangeFormatting => full_or_nothing(!flow),
        Call::SemanticTokens => full_or_nothing(has_keys),
        Call::Formatting => levels,
        Call::Hover => usize::from(!matches!(shape, Shape::QuestionKeys)),
        Call::Completion => usize::from(!anchored),
        Call::CodeLens => 1,
        Call::OnTypeFormatting => usize::from(!flow),
        Call::Definition | Call::PrepareRename | Call::DocumentLink | Call::DocumentColor => {
            usize::from(anchored)
        }
        Call::References | Call::Rename => 2 * usize::from(anchored),
        Call::CodeAction => 0,
    }
}

/// Check that the answer to `call` reaches the bottom of the document, since
/// a handler that stops at the top answers without a stack to measure. For
/// `codeAction`, the conversion the shape allows must be on offer.
fn check(shape: Shape, call: Call, levels: usize, json: &str, formatter: Formatter) {
    let (reached, expected) = (reach(call, json), expected_reach(shape, call, levels));
    assert!(
        reached >= expected,
        "{shape:?} {call:?} {formatter:?}: reach {reached}, expected {expected}: {}",
        &json[..json.len().min(300)]
    );
    if let (Call::CodeAction, Some(conversion)) = (call, shape.conversion(formatter)) {
        assert!(
            json.contains(conversion),
            "{shape:?} {formatter:?}: no {conversion} action in {}",
            &json[..json.len().min(300)]
        );
    }
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| match &diagnostic.code {
            Some(NumberOrString::String(code)) => Some(code.as_str()),
            _ => None,
        })
        .collect()
}

/// The definitions of a schema that nests the composition keywords and
/// `$ref`s once per document level: every mapping value and sequence item
/// goes through `allOf`, `anyOf`, `oneOf` and a `$ref` back to the start. The
/// validator stops at its own depth limit, so only the first levels of a deep
/// document are validated.
fn recursive_schema() -> Value {
    let reference = |name: &str| json!({ "$ref": format!("#/definitions/{name}") });
    json!({
        "allOf": [reference("a")],
        "definitions": {
            "a": { "allOf": [reference("b")] },
            "b": { "anyOf": [reference("c"), { "type": "string" }] },
            "c": { "oneOf": [reference("d"), { "type": "string" }] },
            "d": {
                "properties": {
                    "k": reference("a"),
                    "p": reference("a"),
                    "top": { "type": "integer" },
                    "x": { "type": "integer" },
                    "c": { "type": "string", "contentSchema": reference("a") }
                },
                "additionalProperties": reference("a"),
                "items": reference("a")
            }
        }
    })
}

/// Every validator on, and the schema served by `proxy` for every document.
fn settings(proxy: &FakeProxy, max_nesting_depth: usize, formatter: Formatter) -> Value {
    let enforce = matches!(formatter, Formatter::EnforceBlockStyle);
    json!({
        "maxNestingDepth": max_nesting_depth,
        "httpProxy": proxy.url(),
        "schemaStore": false,
        "schemas": { SCHEMA_URL: "**/*.yaml" },
        "keyOrdering": true,
        "flowStyle": "warning",
        "duplicateKeys": "error",
        "customTags": ["!x"],
        "formatEnforceBlockStyle": enforce,
        "formatRemoveDuplicateKeys": enforce,
    })
}

fn schema_proxy(schema: &Value) -> FakeProxy {
    FakeProxy::start(Reply::ok(Some("application/json"), schema.to_string()))
}

fn schema_fetched(proxy: &FakeProxy) -> bool {
    proxy
        .seen()
        .iter()
        .any(|line| line.starts_with(&format!("CONNECT {SCHEMA_HOST}")))
}

/// Send every request and check that each answers without an error. With
/// `formatter` set, the answers must also reach the bottom of the document,
/// see [`check`].
async fn send_every_request(
    service: &mut LspService<Backend>,
    shape: Shape,
    levels: usize,
    formatter: Option<Formatter>,
) {
    let diagnostics =
        serde_json::to_value(diagnostics_of(service, URI)).expect("diagnostics serialize");
    for (call, request) in requests(shape, levels, &diagnostics) {
        let response = send(service, request)
            .await
            .expect("every request returns a response");
        assert!(response.is_ok(), "{shape:?} {call:?}: {response:?}");
        let result = response
            .result()
            .expect("a successful response has a result")
            .to_string();
        if let Some(formatter) = formatter {
            check(shape, call, levels, &result, formatter);
        }
    }
}

/// Replace and drop the stored AST in every way the server does: a change of
/// the text, a change of the watched files, a limit one below the document
/// (which drops the AST and reports a syntax error) and back, and a close.
async fn churn_the_store(
    service: &mut LspService<Backend>,
    proxy: &FakeProxy,
    text: &str,
    levels: usize,
) {
    send(service, did_change_notification(URI, text, 2)).await;
    send(service, did_change_watched_files_notification(URI)).await;

    let lowered = settings(proxy, levels - 1, Formatter::Default);
    send(service, did_change_configuration_notification(&lowered)).await;
    assert!(codes(&diagnostics_of(service, URI)).contains(&"yamlSyntax"));
    assert!(!has_symbols(service, URI).await);

    let restored = settings(proxy, levels, Formatter::Default);
    send(service, did_change_configuration_notification(&restored)).await;
    assert!(!codes(&diagnostics_of(service, URI)).contains(&"yamlSyntax"));
    assert!(has_symbols(service, URI).await);

    send(service, did_close_notification(URI)).await;
    let response = send(service, document_symbol_request(2, URI))
        .await
        .expect("documentSymbol should return a response");
    assert!(response.is_ok(), "{response:?}");
}

/// Open a document of `levels` levels, with the limit at `levels`, against the
/// recursive schema, and send everything that reads it. A document one level
/// deeper is rejected, which shows that the shape has exactly `levels` levels.
async fn survive_every_request(shape: Shape, levels: usize) {
    let proxy = schema_proxy(&recursive_schema());
    let (mut service, mut log) = start_server(settings(&proxy, levels, Formatter::Default)).await;
    let text = shape.text(levels);

    send(&mut service, did_open_notification(URI, &text)).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;
    assert!(warnings.is_empty(), "{shape:?}: {warnings:?}");
    assert!(schema_fetched(&proxy), "{shape:?}: the schema was not used");
    let diagnostics = diagnostics_of(&service, URI);
    assert!(
        !codes(&diagnostics).contains(&"yamlSyntax"),
        "{shape:?}: the document did not load: {diagnostics:?}"
    );

    send_every_request(&mut service, shape, levels, Some(Formatter::Default)).await;
    let enforcing = settings(&proxy, levels, Formatter::EnforceBlockStyle);
    send(
        &mut service,
        did_change_configuration_notification(&enforcing),
    )
    .await;
    send_every_request(
        &mut service,
        shape,
        levels,
        Some(Formatter::EnforceBlockStyle),
    )
    .await;
    churn_the_store(&mut service, &proxy, &text, levels).await;

    let above = "file:///test/above.yaml";
    send(
        &mut service,
        did_open_notification(above, &shape.text(levels + 1)),
    )
    .await;
    assert_eq!(codes(&diagnostics_of(&service, above)), ["yamlSyntax"]);
}

#[rstest]
#[case::block_mappings(Shape::BlockMappings)]
#[case::block_sequences(Shape::BlockSequences)]
#[case::flow_mappings(Shape::FlowMappings)]
#[case::flow_sequences(Shape::FlowSequences)]
#[case::question_keys(Shape::QuestionKeys)]
#[case::anchor_and_alias(Shape::AnchorAndAlias)]
fn every_ast_request_survives_256_levels_on_2_mib(#[case] shape: Shape) {
    run_on_stack(format!("{shape:?}"), TWO_MIB, move || {
        survive_every_request(shape, DEFAULT_LEVELS)
    });
}

/// `levels` nested block mappings with `top: oops` in the mapping of
/// `marker_level` (0-based), the one value the recursive schema rejects.
fn marked(levels: usize, marker_level: usize) -> String {
    (0..levels)
        .map(|level| {
            let pad = "  ".repeat(level);
            let marker = if level == marker_level {
                [pad.as_str(), "top: oops\n"].concat()
            } else {
                String::new()
            };
            let entry = if level + 1 == levels { "leaf: 1" } else { "k:" };
            [marker.as_str(), pad.as_str(), entry, "\n"].concat()
        })
        .collect()
}

/// `x: oops` and a chain of `levels` levels in all, the innermost document.
fn innermost(levels: usize) -> String {
    let chain: String = (0..levels)
        .map(|level| ["  ".repeat(level).as_str(), "k:\n"].concat())
        .collect();
    format!("x: oops\n{chain}")
}

/// A document with a mapping `c` whose string holds a document with a mapping
/// `c` whose string holds ..., one embedding for each entry of `paddings`,
/// innermost `inner`. An entry is the number of `p` levels in front of the `c`
/// of its document.
fn embedded(paddings: &[usize], inner: &str) -> String {
    paddings
        .iter()
        .rev()
        .fold(inner.to_string(), |content, &padding| {
            let pad = |level: usize| "  ".repeat(level);
            let path: String = (0..padding)
                .map(|level| [pad(level).as_str(), "p:\n"].concat())
                .collect();
            let body: String = content
                .lines()
                .map(|line| [pad(padding + 1).as_str(), line, "\n"].concat())
                .collect();
            format!("{path}{}c: |\n{body}", pad(padding))
        })
}

/// The diagnostics of the schema validation, which compositions that fail
/// report as one diagnostic where the failing composition starts.
fn schema_diagnostics(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(&diagnostic.code, Some(NumberOrString::String(code)) if code.starts_with("schema"))
        })
        .collect()
}

// ---- the schema validator at its depth limit ----

/// The deepest level in which `top: oops` is still validated against the
/// recursive schema: the validator costs four levels of its depth limit for
/// each level of the document, and stops at `MAX_VALIDATION_DEPTH`.
const DEEPEST_VALIDATED_LEVEL: usize = 14;

/// The validator follows the composition keywords down to its depth limit in
/// a document of 256 levels, the deepest stack it uses. A value that the
/// schema rejects fails the compositions above it, so the diagnostic at the
/// root shows that the validator reached the value.
#[rstest]
#[case::last_level_it_reaches(DEEPEST_VALIDATED_LEVEL, 1)]
#[case::first_level_beyond_it(DEEPEST_VALIDATED_LEVEL + 1, 0)]
fn schema_validation_follows_the_compositions_to_its_depth_limit(
    #[case] marker_level: usize,
    #[case] expected_diagnostics: usize,
) {
    run_on_stack(
        "validation-depth".to_string(),
        TWO_MIB,
        move || async move {
            let proxy = schema_proxy(&recursive_schema());
            let (mut service, mut log) =
                start_server(settings(&proxy, DEFAULT_LEVELS, Formatter::Default)).await;

            send(
                &mut service,
                did_open_notification(URI, &marked(DEFAULT_LEVELS, marker_level)),
            )
            .await;

            let warnings = warnings_before_sentinel(&mut service, &mut log).await;
            assert!(warnings.is_empty(), "{warnings:?}");
            assert!(schema_fetched(&proxy));
            let diagnostics = diagnostics_of(&service, URI);
            assert_eq!(
                schema_diagnostics(&diagnostics).len(),
                expected_diagnostics,
                "{diagnostics:?}"
            );
            assert!(has_symbols(&mut service, URI).await);
        },
    );
}

// ---- documents embedded in strings ----

/// A schema whose `contentSchema` validates the YAML embedded in `c`.
fn content_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "c": {
            "type": "string",
            "contentSchema": {
                "type": "object",
                "properties": { "top": { "type": "integer" } }
            }
        } }
    })
}

/// The embedded document keeps the library's default limit, whatever the
/// setting is, so one level above it is reported as a document that cannot be
/// parsed.
#[rstest]
#[case::at_the_default_limit(DEFAULT_LEVELS, "schemaType", "(content)")]
#[case::above_the_default_limit(DEFAULT_LEVELS + 1, "schemaContentSchema", "could not be parsed")]
fn embedded_document_is_loaded_below_the_schema_validator(
    #[case] levels: usize,
    #[case] code: &'static str,
    #[case] message: &'static str,
) {
    run_on_stack("embedded".to_string(), TWO_MIB, move || async move {
        let proxy = schema_proxy(&content_schema());
        let (mut service, mut log) =
            start_server(settings(&proxy, DEFAULT_LEVELS, Formatter::Default)).await;
        let embedded = format!("top: text\n{}", nested(levels));
        let indented: String = embedded
            .lines()
            .map(|line| ["  ", line, "\n"].concat())
            .collect();

        send(
            &mut service,
            did_open_notification(URI, &format!("c: |\n{indented}")),
        )
        .await;

        let warnings = warnings_before_sentinel(&mut service, &mut log).await;
        assert!(warnings.is_empty(), "{warnings:?}");
        let diagnostics = diagnostics_of(&service, URI);
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code == Some(NumberOrString::String(code.to_string()))
                    && diagnostic.message.contains(message)
            }),
            "no {code}: {diagnostics:?}"
        );
        assert!(has_symbols(&mut service, URI).await);
    });
}

/// How a chain of embeddings ends.
#[derive(Clone, Copy, Debug)]
enum ChainEnd {
    /// The innermost value is validated and rejected.
    ReachesInnermost,
    /// The innermost value is deeper than the validator goes.
    CutByDepth,
    /// The ninth embedding stops the validation, which the client is told.
    StoppedByContentLimit,
}

/// The padding of a chain of eight embeddings, in `p` levels before the `c` of
/// each document. Nothing in front of the seven inner embeddings leaves four
/// `p` levels for the first one before the validator's depth limit cuts the
/// innermost value; each `p` level costs four levels of that limit.
fn chain_paddings(end: ChainEnd) -> Vec<usize> {
    match end {
        ChainEnd::ReachesInnermost => [vec![4], vec![0; 7]].concat(),
        ChainEnd::CutByDepth => [vec![5], vec![0; 7]].concat(),
        ChainEnd::StoppedByContentLimit => vec![0; 9],
    }
}

/// Open a chain of embeddings whose innermost string holds a document of 256
/// levels, which the validator loads at the deepest point of its stack, and
/// send everything that reads the outer document.
async fn survive_chain(end: ChainEnd, max_nesting_depth: usize) {
    let proxy = schema_proxy(&recursive_schema());
    let (mut service, mut log) =
        start_server(settings(&proxy, max_nesting_depth, Formatter::Default)).await;
    let text = embedded(&chain_paddings(end), &innermost(DEFAULT_LEVELS));

    send(&mut service, did_open_notification(URI, &text)).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.contains("is incomplete"))
            .count(),
        usize::from(matches!(end, ChainEnd::StoppedByContentLimit)),
        "{end:?}: {warnings:?}"
    );
    assert!(schema_fetched(&proxy), "{end:?}: the schema was not used");
    let diagnostics = diagnostics_of(&service, URI);
    assert_eq!(
        schema_diagnostics(&diagnostics).len(),
        usize::from(matches!(end, ChainEnd::ReachesInnermost)),
        "{end:?}: {diagnostics:?}"
    );
    assert!(has_symbols(&mut service, URI).await);
    send_every_request(&mut service, Shape::BlockMappings, 1, None).await;
}

#[rstest]
#[case::reaches_the_innermost_value(ChainEnd::ReachesInnermost)]
#[case::cut_by_the_depth_limit(ChainEnd::CutByDepth)]
#[case::stopped_by_the_content_limit(ChainEnd::StoppedByContentLimit)]
fn chain_of_embeddings_survives_on_2_mib(#[case] end: ChainEnd) {
    run_on_stack(format!("{end:?}"), TWO_MIB, move || {
        survive_chain(end, DEFAULT_LEVELS)
    });
}

// ---- the schema fetched through spawn_blocking ----

/// The JSON depth limit of a schema, `MAX_JSON_DEPTH` in `src/schema.rs`.
const JSON_DEPTH_LIMIT: usize = 50;

/// The most `additionalProperties` that fit around `{"type": "integer"}`: the
/// string `"integer"` is a level of its own.
const DEEPEST_ACCEPTED_SCHEMA: usize = JSON_DEPTH_LIMIT - 1;

/// A schema that nests `additionalProperties` `levels` times around
/// `{"type": "integer"}`.
fn nested_schema(levels: usize) -> Value {
    (0..levels).fold(
        json!({ "type": "integer" }),
        |inner, _| json!({ "additionalProperties": inner }),
    )
}

/// `levels` nested mappings whose innermost value is a string.
fn string_at(levels: usize) -> String {
    (0..levels)
        .map(|level| {
            let entry = if level + 1 == levels { "k: oops" } else { "k:" };
            ["  ".repeat(level).as_str(), entry, "\n"].concat()
        })
        .collect()
}

/// Open a document against the schema that `proxy` serves, and return the
/// warnings and schema diagnostics. The server answers a request afterwards.
async fn open_against(proxy: &FakeProxy, text: &str) -> (Vec<String>, usize) {
    let (mut service, mut log) =
        start_server(settings(proxy, DEFAULT_LEVELS, Formatter::Default)).await;

    send(&mut service, did_open_notification(URI, text)).await;

    let warnings = warnings_before_sentinel(&mut service, &mut log).await;
    assert!(schema_fetched(proxy));
    assert!(has_symbols(&mut service, URI).await);
    (
        warnings,
        schema_diagnostics(&diagnostics_of(&service, URI)).len(),
    )
}

#[test]
fn schema_at_the_json_depth_limit_is_parsed_and_applied() {
    run_on_stack("deep-schema".to_string(), TWO_MIB, || async {
        let proxy = schema_proxy(&nested_schema(DEEPEST_ACCEPTED_SCHEMA));

        let (warnings, diagnostics) =
            open_against(&proxy, &string_at(DEEPEST_ACCEPTED_SCHEMA)).await;

        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(diagnostics, 1);
    });
}

/// A schema beyond the JSON depth limit is refused, and so is one that the
/// JSON reader refuses for its depth; the server logs the failure and goes on.
#[rstest]
#[case::one_level_beyond_the_limit(nested_schema(DEEPEST_ACCEPTED_SCHEMA + 1).to_string())]
#[case::beyond_the_reader_limit("[".repeat(100_000))]
fn schema_beyond_the_json_depth_limit_is_refused(#[case] body: String) {
    run_on_stack("refused-schema".to_string(), TWO_MIB, move || async move {
        let proxy = FakeProxy::start(Reply::ok(Some("application/json"), body));

        let (warnings, diagnostics) =
            open_against(&proxy, &string_at(DEEPEST_ACCEPTED_SCHEMA)).await;

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(diagnostics, 0);
    });
}

// ---- the upper end of the setting, in a release build ----

/// A release build has to run these: the stack of a debug build is far above
/// the thread's, and the debug build of the same test at 512 levels would not
/// say anything about the shipped server.
fn assert_release_build() {
    #[expect(
        clippy::assertions_on_constants,
        reason = "a debug build must fail with this message, not run the test"
    )]
    {
        assert!(
            !cfg!(debug_assertions),
            "the release runs hold only in a release build"
        );
    }
}

#[rstest]
#[case::block_mappings(Shape::BlockMappings)]
#[case::block_sequences(Shape::BlockSequences)]
#[case::flow_mappings(Shape::FlowMappings)]
#[case::flow_sequences(Shape::FlowSequences)]
#[case::question_keys(Shape::QuestionKeys)]
#[case::anchor_and_alias(Shape::AnchorAndAlias)]
#[ignore = "needs a release build and one process per case (glibc caches thread stacks): \
            cargo test --release -p rlsp-yaml --test lsp_lifecycle -- --ignored --exact \
            --test-threads=1 stack_depth::every_ast_request_survives_512_levels::<case>, \
            with <case> as listed by --list, e.g. case_1_block_mappings"]
fn every_ast_request_survives_512_levels(#[case] shape: Shape) {
    assert_release_build();
    run_on_stack(format!("{shape:?}"), RELEASE_STACK, move || {
        survive_every_request(shape, MAX_LEVELS)
    });
}

#[test]
#[ignore = "needs a release build and a process of its own: \
            cargo test --release -p rlsp-yaml --test lsp_lifecycle -- --ignored --exact \
            --test-threads=1 stack_depth::chain_of_embeddings_survives_with_the_setting_at_512"]
fn chain_of_embeddings_survives_with_the_setting_at_512() {
    assert_release_build();
    run_on_stack("chain-512".to_string(), RELEASE_STACK, || {
        survive_chain(ChainEnd::ReachesInnermost, MAX_LEVELS)
    });
}

// ---- the shipped server, over stdio ----

/// The frames the server writes are `{"jsonrpc":"2.0","result":<result>,"id":<id>}`
/// or the same with `error`. A result can nest deeper than `serde_json` reads
/// back, so the harness cuts the result out of the text instead.
const RESULT_PREFIX: &str = "{\"jsonrpc\":\"2.0\",\"result\":";
const ERROR_PREFIX: &str = "{\"jsonrpc\":\"2.0\",\"error\":";
const INITIALIZE_ID: i64 = 1000;
const SHUTDOWN_ID: i64 = 1001;
const FRAME_TIMEOUT: Duration = Duration::from_secs(60);
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);

/// The compiled `rlsp-yaml` binary, speaking `Content-Length`-framed JSON-RPC
/// over its stdio pipes. A stack overflow in it ends the process, which the
/// harness reports with the exit status. The process is killed on drop.
struct StdioServer {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: mpsc::Receiver<String>,
}

impl Drop for StdioServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_frame(reader: &mut impl Read) -> Option<String> {
    let mut header = Vec::new();
    let mut byte = [0u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        reader.read_exact(&mut byte).ok()?;
        header.push(byte[0]);
    }
    let length: usize = String::from_utf8(header)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length:"))?
        .trim()
        .parse()
        .ok()?;
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    String::from_utf8(body).ok()
}

impl StdioServer {
    /// Start the binary with `options` as `initializationOptions` and finish
    /// the handshake. No proxy of the environment reaches the server: the
    /// schema comes through the one in the options.
    fn start(options: &Value) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rlsp-yaml"));
        for variable in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
        ] {
            command.env_remove(variable);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn the rlsp-yaml binary");
        let stdin = child.stdin.take();
        let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        let (sender, frames) = mpsc::channel();
        std::thread::spawn(move || {
            while let Some(frame) = read_frame(&mut stdout) {
                if sender.send(frame).is_err() {
                    return;
                }
            }
        });
        let mut server = Self {
            child,
            stdin,
            frames,
        };
        server.send(&json!({
            "jsonrpc": "2.0",
            "id": INITIALIZE_ID,
            "method": "initialize",
            "params": {
                "capabilities": {},
                "processId": null,
                "rootUri": null,
                "initializationOptions": options
            }
        }));
        server.result_of(INITIALIZE_ID);
        server.send(&json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
        server
    }

    fn send(&mut self, message: &Value) {
        let body = serde_json::to_vec(message).expect("serialize the message");
        let stdin = self.stdin.as_mut().expect("stdin is open");
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).expect("write the header");
        stdin.write_all(&body).expect("write the body");
        stdin.flush().expect("flush stdin");
    }

    /// The next frame that `accept` takes. A process that ended, or a frame
    /// that does not come, fails the test with the exit status.
    fn next_frame(&mut self, accept: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + FRAME_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let frame = self
                .frames
                .recv_timeout(remaining)
                .map_err(|error| {
                    format!(
                        "{error} within {FRAME_TIMEOUT:?}; exit status {:?}",
                        self.child.try_wait()
                    )
                })
                .expect("a frame from the server");
            if accept(&frame) {
                return frame;
            }
        }
    }

    /// The text of the result of the request `id`.
    fn result_of(&mut self, id: i64) -> String {
        let suffix = format!(",\"id\":{id}}}");
        let frame = self.next_frame(|frame| {
            (frame.starts_with(RESULT_PREFIX) || frame.starts_with(ERROR_PREFIX))
                && frame.ends_with(&suffix)
        });
        assert!(
            frame.starts_with(RESULT_PREFIX),
            "request {id} failed: {frame:.300}"
        );
        frame[RESULT_PREFIX.len()..frame.len() - suffix.len()].to_string()
    }

    fn request(&mut self, request: &Request) -> String {
        let message = serde_json::to_value(request).expect("serialize the request");
        let id = message
            .pointer("/id")
            .and_then(Value::as_i64)
            .expect("a numeric id");
        self.send(&message);
        self.result_of(id)
    }

    /// The diagnostics of the next publication for `uri`.
    fn diagnostics_of(&mut self, uri: &str) -> Value {
        let frame = self.next_frame(|frame| {
            frame.contains("\"method\":\"textDocument/publishDiagnostics\"") && frame.contains(uri)
        });
        let message: Value = serde_json::from_str(&frame).expect("diagnostics are not nested");
        message
            .pointer("/params/diagnostics")
            .cloned()
            .expect("a publication carries diagnostics")
    }

    /// Shut the server down and check that it ends on its own with success.
    fn finish(mut self) {
        self.send(&json!({ "jsonrpc": "2.0", "id": SHUTDOWN_ID, "method": "shutdown" }));
        self.result_of(SHUTDOWN_ID);
        self.send(&json!({ "jsonrpc": "2.0", "method": "exit" }));
        drop(self.stdin.take());
        let deadline = Instant::now() + EXIT_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("wait for the server") {
                assert!(status.success(), "the server ended with {status:?}");
                return;
            }
            assert!(Instant::now() < deadline, "the server did not end");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The shipped server on its own 8 MiB thread, in a debug build, with the
/// setting at its upper end: the same requests and the same checks as the
/// in-process runs, and the schema fetched and parsed on its blocking threads.
#[rstest]
#[case::block_mappings(Shape::BlockMappings)]
#[case::block_sequences(Shape::BlockSequences)]
#[case::flow_mappings(Shape::FlowMappings)]
#[case::flow_sequences(Shape::FlowSequences)]
#[case::question_keys(Shape::QuestionKeys)]
#[case::anchor_and_alias(Shape::AnchorAndAlias)]
fn shipped_server_survives_512_levels_over_stdio(#[case] shape: Shape) {
    let proxy = schema_proxy(&recursive_schema());
    let mut server = StdioServer::start(&settings(&proxy, MAX_LEVELS, Formatter::Default));
    let open = serde_json::to_value(did_open_notification(URI, &shape.text(MAX_LEVELS)))
        .expect("serialize didOpen");
    server.send(&open);

    let diagnostics = server.diagnostics_of(URI);

    assert!(schema_fetched(&proxy), "{shape:?}: the schema was not used");
    assert!(
        !diagnostics.to_string().contains("yamlSyntax"),
        "{shape:?}: the document did not load: {diagnostics}"
    );
    for (call, request) in requests(shape, MAX_LEVELS, &diagnostics) {
        let result = server.request(&request);
        check(shape, call, MAX_LEVELS, &result, Formatter::Default);
    }
    server.finish();
}

#[test]
fn shipped_server_parses_a_schema_at_the_json_depth_limit() {
    let proxy = schema_proxy(&nested_schema(DEEPEST_ACCEPTED_SCHEMA));
    let mut server = StdioServer::start(&settings(&proxy, DEFAULT_LEVELS, Formatter::Default));
    let open = serde_json::to_value(did_open_notification(
        URI,
        &string_at(DEEPEST_ACCEPTED_SCHEMA),
    ))
    .expect("serialize didOpen");
    server.send(&open);

    let diagnostics = server.diagnostics_of(URI);

    assert!(schema_fetched(&proxy));
    assert!(
        diagnostics.to_string().contains("schemaType"),
        "the schema was not applied: {diagnostics}"
    );
    server.finish();
}
