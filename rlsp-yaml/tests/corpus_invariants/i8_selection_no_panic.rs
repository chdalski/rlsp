use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use rlsp_yaml::analysis::selection::selection_ranges;
use tower_lsp::lsp_types::Position;

use super::shared::panic_message;

pub fn check_i8_selection_no_panic(_path: &Path, text: &str) -> Result<(), String> {
    let docs = rlsp_yaml_parser::load(text).unwrap_or_default();
    let pos = Position::new(0, 0);

    let result = catch_unwind(AssertUnwindSafe(|| selection_ranges(&docs, &[pos])))
        .map_err(|e| format!("panic in selection_ranges: {}", panic_message(&e)))?;

    if let Some(sr) = result.first() {
        let mut outermost = sr;
        while let Some(ref p) = outermost.parent {
            outermost = p;
        }
        if outermost.range.start.line != 0 {
            return Err(format!(
                "outermost range start.line is {} (expected 0) for position (0,0)",
                outermost.range.start.line
            ));
        }
    }

    Ok(())
}
