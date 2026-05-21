use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::YamlFormatOptions;
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml::validation::ValidationSettings;
use rlsp_yaml::validation::validators::{
    validate_custom_tags, validate_duplicate_keys, validate_flow_style, validate_key_ordering,
    validate_unused_anchors, validate_yaml11_compat,
};
use tower_lsp::lsp_types::{Position, Range};

use super::shared::{collect_all_diagnostics, panic_message};

pub fn check_i1_no_panics(_path: &Path, text: &str) -> Result<(), String> {
    // Stage 1: parse
    let parse_result = catch_unwind(AssertUnwindSafe(|| parse_yaml(text)))
        .map_err(|e| format!("panic in parse_yaml: {}", panic_message(&e)))?;

    let docs = parse_result.documents;

    // Stage 2: validate_unused_anchors
    catch_unwind(AssertUnwindSafe(|| validate_unused_anchors(&docs)))
        .map_err(|e| format!("panic in validate_unused_anchors: {}", panic_message(&e)))?;

    // Stage 3: validate_flow_style
    catch_unwind(AssertUnwindSafe(|| {
        validate_flow_style(&docs, &ValidationSettings::default())
    }))
    .map_err(|e| format!("panic in validate_flow_style: {}", panic_message(&e)))?;

    // Stage 4: validate_custom_tags (empty allowed set — all tags are unknown)
    catch_unwind(AssertUnwindSafe(|| validate_custom_tags(&docs, &[])))
        .map_err(|e| format!("panic in validate_custom_tags: {}", panic_message(&e)))?;

    // Stage 5: validate_key_ordering
    catch_unwind(AssertUnwindSafe(|| validate_key_ordering(&docs)))
        .map_err(|e| format!("panic in validate_key_ordering: {}", panic_message(&e)))?;

    // Stage 6: validate_duplicate_keys
    catch_unwind(AssertUnwindSafe(|| {
        validate_duplicate_keys(&docs, &ValidationSettings::default())
    }))
    .map_err(|e| format!("panic in validate_duplicate_keys: {}", panic_message(&e)))?;

    // Stage 7: validate_yaml11_compat
    catch_unwind(AssertUnwindSafe(|| validate_yaml11_compat(&docs)))
        .map_err(|e| format!("panic in validate_yaml11_compat: {}", panic_message(&e)))?;

    // Stage 8: format_yaml
    let opts = YamlFormatOptions::default();
    catch_unwind(AssertUnwindSafe(|| {
        rlsp_yaml::editing::formatter::format_yaml(text, &opts)
    }))
    .map_err(|e| format!("panic in format_yaml: {}", panic_message(&e)))?;

    // Stage 9: code_actions with zero-width range at (0,0) and all diagnostics
    let all_diagnostics = collect_all_diagnostics(&docs);
    let zero_range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let fake_uri = tower_lsp::lsp_types::Url::parse("file:///corpus/test.yaml").expect("valid URI");
    catch_unwind(AssertUnwindSafe(|| {
        code_actions(
            &docs,
            text,
            zero_range,
            &all_diagnostics,
            &fake_uri,
            &YamlFormatOptions::default(),
        )
    }))
    .map_err(|e| format!("panic in code_actions: {}", panic_message(&e)))?;

    Ok(())
}
