// SPDX-License-Identifier: MIT
//! Peak heap of schema parses and of schema glob matching, measured with a
//! counting allocator.
//!
//! A schema comes from a URL that a document names, and a parse reads the
//! schema and up to 20 documents that it references with `$ref`, each up to
//! `MAX_SCHEMA_BYTES` (5 MiB) long. These tests build such schemas in their
//! densest forms and check that a parse is rejected before it exhausts memory,
//! and that the largest schema the budgets accept stays below the memory
//! target of 400 MiB. Documents are served over HTTP by the fake proxy of the
//! `lsp_lifecycle` tests, so the fetch path is the production one, including
//! the estimate that is charged before a response is deserialized.
//!
//! The counters are per thread and the parses run on the test's own thread, so
//! the proxy thread and the harness do not disturb a measurement. The tests
//! take a lock anyway, to run one at a time: several multi-hundred-MiB parses at
//! once would multiply the memory of the machine. `cargo nextest` runs each test
//! in a process of its own and does not need the lock.
//!
//! Memory is counted the way glibc allocates it: a request is rounded up to
//! 16 bytes after an 8-byte header, with a minimum chunk of 32 bytes. A
//! million one-byte strings take 32 MiB that way, not 1 MiB.
#![expect(clippy::expect_used, clippy::panic, reason = "test code")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fmt::Write as _;
use std::sync::Mutex;

use rlsp_yaml::schema::{
    FilePatterns, MAX_PATH_BYTES, MAX_PATTERN_BYTES, ParseContext, SchemaCache, SchemaError,
    SizeBudget, fetch_schema_raw, match_schemastore, parse_schema, parse_schemastore_catalog,
};
use serde_json::Value;

#[expect(
    dead_code,
    reason = "shared with the lsp_lifecycle tests, which use the rest of the module"
)]
#[path = "lsp_lifecycle/fake_proxy.rs"]
mod fake_proxy;

use fake_proxy::{FakeProxy, Reply, SCHEMA_HOST};

// ──────────────────────────────────────────────────────────────────────────────
// Counting allocator
// ──────────────────────────────────────────────────────────────────────────────

struct CountingAllocator;

thread_local! {
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

/// The bytes that glibc takes for a request of `size` bytes.
fn chunk(size: usize) -> isize {
    isize::try_from(((size + 8 + 15) & !15).max(32)).unwrap_or(isize::MAX)
}

fn grow(bytes: isize) {
    let live = LIVE.get() + bytes;
    LIVE.set(live);
    if live > PEAK.get() {
        PEAK.set(live);
    }
}

fn shrink(bytes: isize) {
    LIVE.set(LIVE.get() - bytes);
}

// SAFETY: every method forwards to `System`, which upholds the contract of
// `GlobalAlloc`; the counters are plain thread-local cells.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            grow(chunk(layout.size()));
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            grow(chunk(layout.size()));
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        shrink(chunk(layout.size()));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            grow(chunk(new_size));
            shrink(chunk(layout.size()));
        }
        new_ptr
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Run `body` and return its result with the peak heap of this thread above
/// the heap in use before it started.
fn measure<T>(body: impl FnOnce() -> T) -> (T, usize) {
    let before = LIVE.get();
    PEAK.set(before);
    let result = body();
    (result, usize::try_from(PEAK.get() - before).unwrap_or(0))
}

// ──────────────────────────────────────────────────────────────────────────────
// Budgets and targets
// ──────────────────────────────────────────────────────────────────────────────

const MIB: usize = 1 << 20;

/// `MAX_SCHEMA_BYTES` of the server: a response is read up to 5 MiB.
const MAX_DOCUMENT_BYTES: usize = 5 * MIB;

/// The memory target of one parse, including the read buffer.
const PEAK_TARGET: usize = 400 * MIB;

/// The raw budget of one parse: 128 MiB.
const RAW_BUDGET: usize = 128 * MIB;

const ROOT_URL: &str = "http://schema.invalid/root.json";

/// The paths of the 20 documents that the root references: the most a parse
/// fetches.
const DOCUMENT_PATHS: [&str; 20] = [
    "/d01.json",
    "/d02.json",
    "/d03.json",
    "/d04.json",
    "/d05.json",
    "/d06.json",
    "/d07.json",
    "/d08.json",
    "/d09.json",
    "/d10.json",
    "/d11.json",
    "/d12.json",
    "/d13.json",
    "/d14.json",
    "/d15.json",
    "/d16.json",
    "/d17.json",
    "/d18.json",
    "/d19.json",
    "/d20.json",
];

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ──────────────────────────────────────────────────────────────────────────────
// Documents
// ──────────────────────────────────────────────────────────────────────────────

/// A dense form of a schema document: `build(limit)` is the longest document
/// of that form that fits in `limit` bytes.
struct Shape {
    name: &'static str,
    build: Box<dyn Fn(usize) -> String>,
}

impl Shape {
    fn new(name: &'static str, build: impl Fn(usize) -> String + 'static) -> Self {
        Self {
            name,
            build: Box::new(build),
        }
    }
}

/// `head`, then `item(0)`, `item(1)`, ... joined by commas for as long as the
/// document stays within `limit`, then `tail`.
fn repeat_within(
    head: &str,
    mut item: impl FnMut(usize) -> String,
    tail: &str,
    limit: usize,
) -> String {
    let mut document = String::from(head);
    let mut first = true;
    for index in 0.. {
        let next = item(index);
        let separator = usize::from(!first);
        if document.len() + separator + next.len() + tail.len() > limit {
            break;
        }
        if !first {
            document.push(',');
        }
        document.push_str(&next);
        first = false;
    }
    document.push_str(tail);
    document
}

/// A short distinct name for `index`: base 36, lower case.
fn short_name(mut index: usize) -> String {
    let mut name = String::new();
    loop {
        let digit = u32::try_from(index % 36).unwrap_or(0);
        name.push(char::from_digit(digit, 36).unwrap_or('a'));
        index /= 36;
        if index == 0 {
            break;
        }
    }
    name
}

/// Empty schema objects: the cheapest input per schema node.
fn empty_objects(limit: usize) -> String {
    repeat_within(r#"{"allOf":["#, |_| "{}".to_string(), "]}", limit)
}

/// Distinct targets, each referenced once.
fn distinct_targets(limit: usize) -> String {
    // Each target costs a reference and a definition; split the limit evenly.
    let per_part = limit.saturating_sub(40) / 2;
    let references = repeat_within(
        "",
        |i| format!(r##"{{"$ref":"#/$defs/{}"}}"##, short_name(i)),
        "",
        per_part,
    );
    let count = references.matches("$ref").count();
    let definitions = (0..count)
        .map(|i| format!(r#""{}":{{}}"#, short_name(i)))
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"allOf":[{references}],"$defs":{{{definitions}}}}}"#)
}

/// Anchors at the bottom of chains of schemas nested 45 levels deep, and one
/// reference to an anchor, which makes the parse index them.
fn deep_anchors(limit: usize) -> String {
    const LEVELS: usize = 45;
    let mut counter = 0;
    let unit = |counter: &mut usize| {
        let mut chain = String::new();
        for _ in 0..LEVELS {
            write!(chain, r#"{{"$anchor":"{}","not":"#, short_name(*counter))
                .expect("writing to a String");
            *counter += 1;
        }
        chain.push_str("{}");
        chain.push_str(&"}".repeat(LEVELS));
        chain
    };
    repeat_within(
        r##"{"allOf":[{"$ref":"#0"},"##,
        |_| unit(&mut counter),
        "]}",
        limit,
    )
}

/// A `required` list of one-character strings.
fn required_strings(limit: usize) -> String {
    repeat_within(r#"{"required":["#, |_| r#""a""#.to_string(), "]}", limit)
}

/// An `enum` of small values.
fn enum_of_small_values(limit: usize) -> String {
    repeat_within(r#"{"enum":["#, |_| "0".to_string(), "]}", limit)
}

/// An array of one-entry objects in an `enum`.
fn enum_of_one_entry_objects(limit: usize) -> String {
    repeat_within(r#"{"enum":["#, |_| r#"{"a":1}"#.to_string(), "]}", limit)
}

/// `item` repeated under a keyword that no schema reads: only the raw budget
/// is spent.
fn unread_padding(limit: usize, item: &str) -> String {
    repeat_within(r#"{"x-pad":["#, |_| item.to_string(), "]}", limit)
}

/// Properties with long distinct names and descriptions.
fn named_properties(limit: usize) -> String {
    repeat_within(
        r#"{"properties":{"#,
        |i| format!(r#""{i:0>64}":{{"description":"{}"}}"#, "d".repeat(64)),
        "}}",
        limit,
    )
}

/// The dense forms that spend the parsed budget, or both budgets.
fn shapes() -> Vec<Shape> {
    vec![
        Shape::new("empty objects", empty_objects),
        Shape::new("distinct targets", distinct_targets),
        Shape::new("deep anchors", deep_anchors),
        Shape::new("required strings", required_strings),
        Shape::new("enum of small values", enum_of_small_values),
        Shape::new("enum of one-entry objects", enum_of_one_entry_objects),
        Shape::new("named properties", named_properties),
        Shape::new("unread one-entry objects", |limit| {
            unread_padding(limit, r#"{"a":1}"#)
        }),
    ]
}

/// The dense forms of text that no schema reads, which only the raw budget
/// bounds, one for each kind of JSON value.
fn padding_shapes() -> Vec<Shape> {
    [
        ("padding of one-entry objects", r#"{"a":1}"#),
        ("padding of empty objects", "{}"),
        ("padding of empty arrays", "[]"),
        ("padding of one-element arrays", "[0]"),
        ("padding of nested arrays", "[[0]]"),
        ("padding of small numbers", "0"),
        ("padding of short strings", r#""a""#),
        ("padding of objects of arrays", r#"{"a":[0]}"#),
    ]
    .into_iter()
    .map(|(name, item)| Shape::new(name, move |limit| unread_padding(limit, item)))
    .collect()
}

/// The root schema: a reference to each of the 20 documents.
fn root_document() -> String {
    let references: Vec<String> = DOCUMENT_PATHS
        .iter()
        .map(|path| format!(r#"{{"$ref":"http://{SCHEMA_HOST}{path}"}}"#))
        .collect();
    format!(r#"{{"allOf":[{}]}}"#, references.join(","))
}

// ──────────────────────────────────────────────────────────────────────────────
// Probes
// ──────────────────────────────────────────────────────────────────────────────

/// What fetching and parsing the root schema, with 20 documents of `shape` of
/// up to `limit` bytes each behind it, came to.
struct Probe {
    /// The budget that rejected the parse, or `None` when it was accepted.
    exceeded: Option<SizeBudget>,
    /// The peak heap above the heap before the fetch.
    peak: usize,
    /// How many documents the server was asked for.
    documents_requested: usize,
    /// Whether the cache holds a document afterwards.
    cache_is_empty: bool,
}

fn probe(shape: &Shape, limit: usize) -> Probe {
    let document = (shape.build)(limit);
    let json = |body: String| Reply::ok(Some("application/json"), body);
    let mut routes: Vec<(&'static str, Reply)> = vec![("/root.json", json(root_document()))];
    routes.extend(
        DOCUMENT_PATHS
            .iter()
            .map(|path| (*path, json(document.clone()))),
    );
    let proxy = FakeProxy::start_routes(routes);
    let proxy_url = proxy.url();
    let mut cache = SchemaCache::new();

    let (result, peak) = measure(|| {
        let mut ctx = ParseContext::new(&mut cache, Some(&proxy_url));
        fetch_schema_raw(ROOT_URL, Some(&proxy_url), Some(&mut ctx)).map(|_| ())
    });

    let exceeded = match result {
        Ok(()) => None,
        Err(SchemaError::TooLarge(budget)) => Some(budget),
        Err(other) => panic!("{}: unexpected error: {other}", shape.name),
    };
    let documents_requested = proxy
        .seen()
        .iter()
        .filter(|line| line.starts_with("GET /d"))
        .count();
    Probe {
        exceeded,
        peak,
        documents_requested,
        cache_is_empty: !DOCUMENT_PATHS
            .iter()
            .any(|path| cache.contains_document(&format!("http://{SCHEMA_HOST}{path}"))),
    }
}

const fn mib(bytes: usize) -> usize {
    bytes / MIB
}

#[test]
fn full_size_documents_of_each_shape_are_rejected_within_the_memory_target() {
    let _guard = one_at_a_time();
    let mut failures = Vec::new();

    for shape in &shapes() {
        let probe = probe(shape, MAX_DOCUMENT_BYTES);
        eprintln!(
            "{:<28} rejected by {:?}, peak {} MiB, {} documents requested",
            shape.name,
            probe.exceeded,
            mib(probe.peak),
            probe.documents_requested
        );
        if probe.exceeded.is_none() {
            failures.push(format!("{}: accepted", shape.name));
        }
        if probe.peak > PEAK_TARGET {
            failures.push(format!("{}: peak {} MiB", shape.name, mib(probe.peak)));
        }
        if !probe.cache_is_empty {
            failures.push(format!("{}: a fetched document was kept", shape.name));
        }
        if probe.documents_requested >= DOCUMENT_PATHS.len() {
            failures.push(format!("{}: fetching went on after the budget", shape.name));
        }
    }

    assert!(failures.is_empty(), "{failures:#?}");
}

/// The largest document length, in bytes, for which the 20 documents of `shape`
/// are accepted, with the highest peak among the accepted probes.
fn largest_accepted(shape: &Shape) -> (usize, usize) {
    let (mut accepted, mut rejected) = (0, MAX_DOCUMENT_BYTES);
    let mut highest_peak = 0;
    while rejected - accepted > (accepted / 40).max(256) {
        let middle = accepted + (rejected - accepted) / 2;
        let probe = probe(shape, middle);
        if probe.exceeded.is_none() {
            accepted = middle;
            highest_peak = highest_peak.max(probe.peak);
        } else {
            rejected = middle;
        }
    }
    (accepted, highest_peak)
}

#[test]
fn largest_accepted_documents_of_each_shape_stay_within_the_memory_target() {
    let _guard = one_at_a_time();
    let mut failures = Vec::new();

    for shape in &shapes() {
        let (length, peak) = largest_accepted(shape);
        eprintln!(
            "{:<28} accepted up to {} KiB per document, peak {} MiB",
            shape.name,
            length / 1024,
            mib(peak)
        );
        if peak > PEAK_TARGET {
            failures.push(format!("{}: peak {} MiB", shape.name, mib(peak)));
        }
    }

    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn raw_budget_rejects_documents_that_deserialize_to_more_than_it_allows() {
    let _guard = one_at_a_time();
    let mut failures = Vec::new();

    for shape in &padding_shapes() {
        // Measure what a sample deserializes to, then scale the 20 documents
        // so that together they take 102 % of the raw budget.
        let sample = (shape.build)(MAX_DOCUMENT_BYTES / 16);
        let (value, heap) = measure(|| serde_json::from_str::<Value>(&sample).expect("JSON"));
        drop(value);
        let per_document = RAW_BUDGET / 20 * 102 / 100;
        let length = sample.len() * per_document / heap;
        if length > MAX_DOCUMENT_BYTES {
            failures.push(format!("{}: needs {length} bytes per document", shape.name));
            continue;
        }

        let probe = probe(shape, length);
        eprintln!(
            "{:<30} {} KiB documents, {} MiB of heap each, rejected by {:?}",
            shape.name,
            length / 1024,
            mib(heap * length / sample.len()),
            probe.exceeded
        );
        if probe.exceeded != Some(SizeBudget::Raw) {
            failures.push(format!("{}: {:?}", shape.name, probe.exceeded));
        }
    }

    assert!(failures.is_empty(), "{failures:#?}");
}

// ──────────────────────────────────────────────────────────────────────────────
// Real schemas
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn real_schemas_parse_within_the_memory_target() {
    let _guard = one_at_a_time();
    let fixtures = [
        (
            "draft-07",
            include_str!("fixtures/schemas/json-schema-draft-07.json"),
        ),
        (
            "cloud-init",
            include_str!("fixtures/schemas/cloud-init.json"),
        ),
        (
            "github-workflow",
            include_str!("fixtures/schemas/github-workflow.json"),
        ),
        (
            "compose-spec",
            include_str!("fixtures/schemas/compose-spec.json"),
        ),
        (
            "kubernetes-deployment",
            include_str!("fixtures/schemas/kubernetes-deployment-apps-v1.json"),
        ),
    ];
    for (name, text) in fixtures {
        let (result, peak) = measure(|| {
            let value: Value = serde_json::from_str(text).expect("fixture is JSON");
            parse_schema(&value).map(|_| ())
        });
        eprintln!("{name}: peak {} KiB", peak / 1024);
        result.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(peak <= PEAK_TARGET, "{name}: {} MiB", mib(peak));
    }
}

#[test]
fn parse_schema_applies_the_production_budgets() {
    let _guard = one_at_a_time();
    // Too many nodes for the parsed budget, few enough for the raw one.
    let nodes = vec![Value::Object(serde_json::Map::new()); 150_000];
    let schema = serde_json::json!({ "allOf": nodes });

    let (result, peak) = measure(|| parse_schema(&schema).map(|_| ()));

    assert!(
        matches!(result, Err(SchemaError::TooLarge(SizeBudget::Parsed))),
        "{result:?}"
    );
    assert!(peak <= PEAK_TARGET, "{} MiB", mib(peak));
}

// ──────────────────────────────────────────────────────────────────────────────
// Glob matching
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn measuring_the_heap_sees_an_allocation() {
    let _guard = one_at_a_time();
    let (_, peak) = measure(|| std::hint::black_box(vec![0_u8; 64]));
    assert!(peak > 0);
}

#[test]
fn matching_a_schemas_pattern_allocates_no_heap() {
    let _guard = one_at_a_time();
    // The longest pattern: MAX_PATTERN_BYTES tokens, the leading `**/` and the
    // end state fill the stack buffers of the matcher.
    let longest = "a".repeat(MAX_PATTERN_BYTES - 2) + "*b";
    let patterns = FilePatterns::new(&[longest, "*.yaml".to_string(), "!skip/*.yaml".to_string()])
        .expect("the patterns include files");
    let long_path = "/".to_string() + &"d/".repeat(MAX_PATH_BYTES / 2 - 3) + "x.yml";
    assert_eq!(long_path.len(), MAX_PATH_BYTES);
    let paths = [
        // a match of the longest pattern, by the simulation
        (
            "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES - 2) + "zzb",
            true,
        ),
        // a match of a short pattern
        ("/d/x.yaml".to_string(), true),
        // an exclusion
        ("/d/skip/x.yaml".to_string(), false),
        // no match, rejected by the trailing literal
        ("/d/x.yml".to_string(), false),
        // no match, found by the simulation
        (
            "/d/".to_string() + &"a".repeat(MAX_PATTERN_BYTES - 3) + "b",
            false,
        ),
        // the longest path
        (long_path, false),
    ];

    let (results, peak) = measure(|| paths.each_ref().map(|(path, _)| patterns.matches(path)));

    assert_eq!(results, paths.each_ref().map(|(_, expected)| *expected));
    assert_eq!(peak, 0);
}

#[test]
fn matching_a_catalog_glob_beyond_a_schemas_pattern_takes_its_states_from_the_heap() {
    let _guard = one_at_a_time();
    let glob = "a".repeat(MAX_PATTERN_BYTES + 44) + "*b.yaml";
    let catalog = parse_schemastore_catalog(&serde_json::json!({
        "schemas": [{ "url": "https://example.com/s.json", "fileMatch": [glob] }]
    }))
    .expect("the catalog parses");
    let matching = "a".repeat(MAX_PATTERN_BYTES + 44) + "zzb.yaml";
    let simulated_miss = "a".repeat(MAX_PATTERN_BYTES + 43) + "b.yaml";
    let rejected_miss = "a".repeat(MAX_PATTERN_BYTES + 44) + "zzb.yml";

    let (matched, _) = measure(|| match_schemastore(&matching, &catalog));
    let (simulated, simulated_peak) = measure(|| match_schemastore(&simulated_miss, &catalog));
    let (rejected, rejected_peak) = measure(|| match_schemastore(&rejected_miss, &catalog));

    assert_eq!(matched.as_deref(), Some("https://example.com/s.json"));
    assert_eq!(simulated, None);
    assert_eq!(rejected, None);
    assert!(simulated_peak > 0, "the simulation did not run");
    assert_eq!(rejected_peak, 0, "the trailing literal rejects first");
}
