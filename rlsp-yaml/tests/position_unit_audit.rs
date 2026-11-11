// SPDX-License-Identifier: MIT
//
// Source audit: the features of this crate count columns in code points, and
// only `src/position_encoding.rs` counts the UTF-16 code units a client counts.
// A feature that counts UTF-16 units itself mixes units with the columns the
// parser provides, and the handlers would convert its positions twice.

#![expect(missing_docs, reason = "test code")]
#![expect(
    clippy::expect_used,
    reason = "test code — expect on infallible filesystem operations"
)]
use std::fs;
use std::path::Path;

const UTF16_COUNTERS: [&str; 2] = ["len_utf16", "encode_utf16"];

fn collect_uses(dir: &Path, base: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read_dir failed") {
        let path = entry.expect("DirEntry failed").path();
        if path.is_dir() {
            collect_uses(&path, base, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let rel = path.strip_prefix(base).expect("path is under base");
            if rel == Path::new("position_encoding.rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("read_to_string failed");
            for (number, line) in source.lines().enumerate() {
                if UTF16_COUNTERS.iter().any(|counter| line.contains(counter)) {
                    out.push(format!("{}:{}: {}", rel.display(), number + 1, line.trim()));
                }
            }
        }
    }
}

#[test]
fn only_the_position_encoding_counts_utf16_units() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut uses = Vec::new();

    collect_uses(&src, &src, &mut uses);

    assert!(
        uses.is_empty(),
        "count UTF-16 units only in src/position_encoding.rs:\n{}",
        uses.join("\n")
    );
}
