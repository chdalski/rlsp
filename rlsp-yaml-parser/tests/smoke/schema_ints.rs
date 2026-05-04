use super::*;
use rlsp_yaml_parser::loader::LoaderBuilder;

// -----------------------------------------------------------------------
// Group Z — §10.3.2 sign-prefix constraint for octal/hex integers
//
// The YAML 1.2.2 §10.3.2 Core schema table places `[-+]?` only on the
// decimal int row.  Signed octal/hex (`-0o10`, `+0xFF`) match no int row
// and must resolve to !!str.
// -----------------------------------------------------------------------

fn core_scalar_tag(input: &str) -> Option<String> {
    let result = LoaderBuilder::new()
        .schema(Schema::Core)
        .build()
        .load(input);
    let docs = result.unwrap_or_else(|e| unreachable!("load must succeed, got: {e}"));
    assert_eq!(docs.len(), 1, "expected exactly one document");
    let first = docs
        .into_iter()
        .next()
        .unwrap_or_else(|| unreachable!("docs is non-empty"));
    match first.root {
        Node::Scalar { tag, .. } => tag.map(std::borrow::Cow::into_owned),
        other @ (Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. }) => {
            unreachable!("expected root scalar, got: {other:?}")
        }
    }
}

// Z-1: `-0o10` — signed octal must resolve to !!str, not !!int
#[test]
fn signed_octal_negative_resolves_to_str() {
    let tag = core_scalar_tag("-0o10\n");
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:str"),
        "signed octal `-0o10` must resolve to !!str under Core schema"
    );
}

// Z-2: `+0o10` — signed octal must resolve to !!str, not !!int
#[test]
fn signed_octal_positive_resolves_to_str() {
    let tag = core_scalar_tag("+0o10\n");
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:str"),
        "signed octal `+0o10` must resolve to !!str under Core schema"
    );
}

// Z-3: `-0xFF` — signed hex must resolve to !!str, not !!int
#[test]
fn signed_hex_negative_resolves_to_str() {
    let tag = core_scalar_tag("-0xFF\n");
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:str"),
        "signed hex `-0xFF` must resolve to !!str under Core schema"
    );
}

// Z-4: `+0xFF` — signed hex must resolve to !!str, not !!int
#[test]
fn signed_hex_positive_resolves_to_str() {
    let tag = core_scalar_tag("+0xFF\n");
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:str"),
        "signed hex `+0xFF` must resolve to !!str under Core schema"
    );
}

// Z-5: `0o10` — unsigned octal must still resolve to !!int (regression guard)
#[test]
fn unsigned_octal_still_resolves_to_int() {
    let tag = core_scalar_tag("0o10\n");
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:int"),
        "unsigned octal `0o10` must still resolve to !!int under Core schema"
    );
}

// Z-6: `0xFF` — unsigned hex must still resolve to !!int (regression guard)
#[test]
fn unsigned_hex_still_resolves_to_int() {
    let tag = core_scalar_tag("0xFF\n");
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:int"),
        "unsigned hex `0xFF` must still resolve to !!int under Core schema"
    );
}

// Z-7: `-42` — signed decimal must still resolve to !!int (regression guard)
#[test]
fn signed_decimal_negative_still_resolves_to_int() {
    let tag = core_scalar_tag("-42\n");
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:int"),
        "signed decimal `-42` must still resolve to !!int under Core schema"
    );
}

// Z-8: `+42` — signed decimal must still resolve to !!int (regression guard)
#[test]
fn signed_decimal_positive_still_resolves_to_int() {
    let tag = core_scalar_tag("+42\n");
    assert_eq!(
        tag.as_deref(),
        Some("tag:yaml.org,2002:int"),
        "signed decimal `+42` must still resolve to !!int under Core schema"
    );
}
