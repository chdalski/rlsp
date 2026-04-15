//! Loader conformance diagnostic binary.
//!
//! Uses the same `parse_test_metadata` from tests/conformance/main.rs
//! but reimplemented inline.

#![expect(
    clippy::unwrap_used,
    reason = "diagnostic binary — not production code"
)]

use rlsp_yaml_parser::loader::load;
use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{CollectionStyle, ScalarStyle, Span, parse_events};
use std::fs;
use std::path::PathBuf;

fn visual_to_raw(s: &str) -> String {
    s.replace('␣', " ")
        .replace('»', "\t")
        .replace('—', "")
        .replace('←', "\r")
        .replace('\u{21D4}', "\u{FEFF}")
        .replace('↵', "")
        .replace("∎\n", "")
}

fn print_events(yaml: &str) {
    for result in parse_events(yaml) {
        match result {
            Ok((event, _span)) => println!("  EVENT: {event:?}"),
            Err(e) => println!("  ERROR: {e}"),
        }
    }
}

fn print_node(node: &Node<Span>, indent: usize) {
    let pad = " ".repeat(indent);
    match node {
        Node::Scalar {
            value,
            style,
            anchor,
            tag,
            ..
        } => {
            let style_ch = match style {
                ScalarStyle::Plain => ':',
                ScalarStyle::SingleQuoted => '\'',
                ScalarStyle::DoubleQuoted => '"',
                ScalarStyle::Literal(_) => '|',
                ScalarStyle::Folded(_) => '>',
            };
            let anchor_str = anchor
                .as_deref()
                .map_or(String::new(), |a| format!("&{a} "));
            let tag_str = tag.as_deref().map_or(String::new(), |t| format!("<{t}> "));
            let value_escaped = value.replace('\n', "\\n").replace('\t', "\\t");
            println!("{pad}=VAL {anchor_str}{tag_str}{style_ch}{value_escaped}");
        }
        Node::Alias { name, .. } => {
            println!("{pad}=ALI *{name}");
        }
        Node::Sequence {
            items,
            style,
            anchor,
            tag,
            ..
        } => {
            let style_ch = if *style == CollectionStyle::Flow {
                " []"
            } else {
                ""
            };
            let anchor_str = anchor
                .as_deref()
                .map_or(String::new(), |a| format!("&{a} "));
            let tag_str = tag.as_deref().map_or(String::new(), |t| format!("<{t}> "));
            println!("{pad}+SEQ{style_ch} {anchor_str}{tag_str}");
            for item in items {
                print_node(item, indent + 1);
            }
            println!("{pad}-SEQ");
        }
        Node::Mapping {
            entries,
            style,
            anchor,
            tag,
            ..
        } => {
            let style_ch = if *style == CollectionStyle::Flow {
                " {}"
            } else {
                ""
            };
            let anchor_str = anchor
                .as_deref()
                .map_or(String::new(), |a| format!("&{a} "));
            let tag_str = tag.as_deref().map_or(String::new(), |t| format!("<{t}> "));
            println!("{pad}+MAP{style_ch} {anchor_str}{tag_str}");
            for (k, v) in entries {
                print_node(k, indent + 1);
                print_node(v, indent + 1);
            }
            println!("{pad}-MAP");
        }
    }
}

fn print_docs(docs: &[Document<Span>]) {
    println!("+STR");
    for doc in docs {
        println!(" +DOC");
        print_node(&doc.root, 2);
        println!(" -DOC");
    }
    println!("-STR");
}

// ---- minimal test metadata parser -----

#[derive(Default)]
struct EntryFields {
    name: Option<String>,
    yaml: Option<String>,
    tree: Option<String>,
    fail: Option<bool>,
    skip: bool,
}

impl EntryFields {
    fn set(&mut self, key: &str, value: &str) {
        match key {
            "name" => self.name = Some(value.to_string()),
            "yaml" | "yaml2" => self.yaml = Some(value.to_string()),
            "fail" => self.fail = Some(value == "true"),
            "skip" => self.skip = true,
            _ => {}
        }
    }
    fn set_block(&mut self, key: &str, block: String) {
        match key {
            "name" => self.name = Some(block),
            "yaml" | "yaml2" => self.yaml = Some(block),
            "tree" => self.tree = Some(block),
            "fail" => self.fail = Some(block.trim() == "true"),
            "skip" => self.skip = true,
            _ => {}
        }
    }
}

struct TestEntry {
    name: String,
    yaml: String,
    tree: Option<String>,
    fail: bool,
}

fn flush_block(
    current: &mut EntryFields,
    block_key: &mut Option<String>,
    block_buf: &mut Option<String>,
) {
    if let (Some(k), Some(b)) = (block_key.take(), block_buf.take()) {
        current.set_block(&k, b);
    }
}

fn flush_entry(
    current: &mut EntryFields,
    inherited: &mut EntryFields,
    results: &mut Vec<TestEntry>,
) {
    if let Some(n) = current.name.take() {
        inherited.name = Some(n);
    }
    if let Some(y) = current.yaml.take() {
        inherited.yaml = Some(y);
    }
    if let Some(t) = current.tree.take() {
        inherited.tree = Some(t);
    }
    if current.skip {
        inherited.skip = true;
    }
    let fail = current.fail.take().unwrap_or(false);
    if !inherited.skip {
        if let Some(ref yaml) = inherited.yaml {
            results.push(TestEntry {
                name: inherited.name.clone().unwrap_or_default(),
                yaml: yaml.clone(),
                tree: inherited.tree.clone(),
                fail,
            });
        }
    }
    inherited.tree = None;
}

fn parse_field(
    line: &str,
    current: &mut EntryFields,
    block_key: &mut Option<String>,
    block_buf: &mut Option<String>,
) {
    if let Some((key, value)) = line.split_once(": ") {
        let key = key.trim();
        let value = value.trim();
        let is_block_scalar = value == "|"
            || (value.starts_with('|')
                && value.len() == 2
                && value.as_bytes().get(1).is_some_and(u8::is_ascii_digit));
        if is_block_scalar {
            // Block scalar with optional indentation indicator
            *block_key = Some(key.to_string());
            *block_buf = Some(String::new());
        } else {
            current.set(key, value);
        }
    } else if let Some(key) = line.strip_suffix(": ") {
        // empty value
        current.set(key.trim(), "");
    }
}

fn parse_test_metadata(content: &str) -> Vec<TestEntry> {
    let mut results: Vec<TestEntry> = Vec::new();
    let mut inherited = EntryFields::default();
    let mut current = EntryFields::default();
    let mut block_key: Option<String> = None;
    let mut block_buf: Option<String> = None;
    let mut in_entry = false;

    for line in content.lines() {
        if line == "---" {
            continue;
        }
        if let Some(rest) = line.strip_prefix("- ") {
            flush_block(&mut current, &mut block_key, &mut block_buf);
            if in_entry {
                flush_entry(&mut current, &mut inherited, &mut results);
            }
            in_entry = true;
            current = EntryFields::default();
            parse_field(rest, &mut current, &mut block_key, &mut block_buf);
        } else if block_buf.is_some() {
            if let Some(indented) = line.strip_prefix("    ") {
                block_buf.as_mut().unwrap().push_str(indented);
                block_buf.as_mut().unwrap().push('\n');
            } else if line.trim_matches([' ', '\t']).is_empty() {
                block_buf.as_mut().unwrap().push('\n');
            } else {
                flush_block(&mut current, &mut block_key, &mut block_buf);
                if line.starts_with("  ") && !line.starts_with("    ") {
                    parse_field(line.trim(), &mut current, &mut block_key, &mut block_buf);
                }
            }
        } else if line.starts_with("  ") && !line.starts_with("    ") {
            flush_block(&mut current, &mut block_key, &mut block_buf);
            parse_field(line.trim(), &mut current, &mut block_key, &mut block_buf);
        }
    }

    flush_block(&mut current, &mut block_key, &mut block_buf);
    if in_entry {
        flush_entry(&mut current, &mut inherited, &mut results);
    }
    results
}

fn get_test_entry(stem: &str, idx: usize) -> Option<TestEntry> {
    let test_dir = PathBuf::from("/workspace/tests/yaml-test-suite/src");
    let file = test_dir.join(format!("{stem}.yaml"));
    let content = fs::read_to_string(file).ok()?;
    let mut entries = parse_test_metadata(&content);
    if idx < entries.len() {
        Some(entries.remove(idx))
    } else {
        None
    }
}

fn diagnose(stem: &str, idx: usize) {
    let Some(entry) = get_test_entry(stem, idx) else {
        println!("=== {stem}[{idx}] NOT FOUND ===");
        return;
    };

    let yaml_raw = visual_to_raw(&entry.yaml);

    println!("\n=== {}[{}] — {} ===", stem, idx, entry.name);
    if entry.fail {
        println!("  [fail case]");
        return;
    }

    println!("YAML bytes: {:?}", &yaml_raw[..yaml_raw.len().min(200)]);

    println!("\nEvents:");
    print_events(&yaml_raw);

    println!("\nAST:");
    match load(&yaml_raw) {
        Ok(docs) => print_docs(&docs),
        Err(e) => println!("ERROR: {e}"),
    }

    if let Some(tree) = &entry.tree {
        println!("\nExpected tree:\n{tree}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if let (Some(stem), Some(idx_str)) = (args.get(1), args.get(2)) {
        let idx: usize = idx_str.parse().unwrap_or(0);
        diagnose(stem, idx);
        return;
    }

    // Default: diagnose a set of interesting cases
    let cases = [
        // Indentation issues
        ("2AUY", 0),
        ("4RWC", 0),
        ("6CA3", 0),
        ("Q5MG", 0),
        // Anchor placement
        ("26DV", 0),
        ("ZWK4", 0),
        // Scalar value issues
        ("36F6", 0),
        ("9YRD", 0),
    ];

    for (stem, idx) in &cases {
        diagnose(stem, *idx);
    }
}
