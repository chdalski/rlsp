use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use rlsp_yaml::navigation::references::{find_references, goto_definition};
use rlsp_yaml_parser::{Node, Span};
use tower_lsp::lsp_types::Position;

use super::shared::panic_message;

pub fn check_i6_tag_loc_invariant(_path: &Path, text: &str) -> Result<(), String> {
    let Ok(docs) = rlsp_yaml_parser::loader::load(text) else {
        return Ok(()); // invalid YAML has no AST to check
    };
    for doc in &docs {
        check_i6_node(&doc.root)?;
    }
    Ok(())
}

pub fn check_i6_node(node: &Node<Span>) -> Result<(), String> {
    match node {
        Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
            // Resolver-injected core schema tags (`tag:yaml.org,2002:*`) have no source
            // position (`tag_loc: None`) by design — they were inferred, not written in
            // the source.  Allow those through.  Any other tag that is present must have
            // a corresponding source location.
            let tag_loc = node.tag_loc();
            let is_resolver_injected = tag
                .as_deref()
                .is_some_and(|t| t.starts_with("tag:yaml.org,2002:"));
            if tag.is_some() && tag_loc.is_none() && !is_resolver_injected {
                return Err(format!(
                    "I6 invariant violated: tag={tag:?} but tag_loc={tag_loc:?}"
                ));
            }
        }
        Node::Alias { .. } => {}
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                check_i6_node(k)?;
                check_i6_node(v)?;
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_i6_node(item)?;
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
    Ok(())
}

pub fn check_i6_references_no_panics(path: &Path, text: &str) -> Result<(), String> {
    let docs = rlsp_yaml_parser::load(text).unwrap_or_default();
    let last_line = text.lines().count().saturating_sub(1) as u32;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let fake_uri = tower_lsp::lsp_types::Url::parse(&format!("file:///corpus/{file_name}"))
        .expect("valid URI");

    for line in [0u32, last_line] {
        let pos = Position::new(line, 0);
        catch_unwind(AssertUnwindSafe(|| goto_definition(&docs, &fake_uri, pos))).map_err(|e| {
            format!(
                "panic in goto_definition at line {line}: {}",
                panic_message(&e)
            )
        })?;
        catch_unwind(AssertUnwindSafe(|| {
            find_references(&docs, &fake_uri, pos, false)
        }))
        .map_err(|e| {
            format!(
                "panic in find_references at line {line}: {}",
                panic_message(&e)
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::path::Path;

    use rlsp_yaml_parser::{Node, ScalarStyle, Span};

    use super::*;

    // UT-I6-1: plain mapping YAML — resolver injects tag:yaml.org,2002:map with
    // no tag_loc.  The narrowed I6 assertion must pass for this case.
    #[test]
    fn i6_resolver_injected_tag_no_tag_loc_passes() {
        let result = check_i6_tag_loc_invariant(Path::new("test.yaml"), "key: value\n");
        assert!(
            result.is_ok(),
            "resolver-injected core tag with tag_loc=None should pass I6: {result:?}"
        );
    }

    // UT-I6-2: explicit user tag on a scalar — tag_loc is Some (source position
    // from the `!custom` token).  The invariant must pass.
    #[test]
    fn i6_explicit_user_tag_with_tag_loc_passes() {
        let result = check_i6_tag_loc_invariant(Path::new("test.yaml"), "!custom value\n");
        assert!(
            result.is_ok(),
            "explicit user tag with tag_loc=Some should pass I6: {result:?}"
        );
    }

    // UT-I6-3: synthetically constructed node with a non-core tag but no tag_loc —
    // simulates a hypothetical loader bug.  The narrowed assertion must still catch
    // this case.
    #[test]
    fn i6_missing_tag_loc_for_non_core_tag_fails() {
        let origin = Span { start: 0, end: 0 };
        let node = Node::Scalar {
            value: String::new(),
            style: ScalarStyle::Plain,
            tag: Some(Cow::Owned("!custom".to_owned())),
            loc: origin,
            // Simulated loader bug: user tag with no source position (meta: None).
            meta: None,
        };
        let result = check_i6_node(&node);
        assert!(
            result.is_err(),
            "non-core tag with tag_loc=None should fail I6"
        );
    }

    // UT-I6-4: no tag, no tag_loc — the zero-tag baseline must pass I6.
    #[test]
    fn i6_no_tag_no_tag_loc_passes() {
        let result = check_i6_tag_loc_invariant(Path::new("test.yaml"), "key: value\n");
        assert!(
            result.is_ok(),
            "node with no tag and no tag_loc should pass I6: {result:?}"
        );
    }
}
