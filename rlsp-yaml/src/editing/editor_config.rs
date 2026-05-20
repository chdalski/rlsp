// SPDX-License-Identifier: MIT

//! `.editorconfig` resolution for open documents.
//!
//! This module reads `.editorconfig` files by walking up from each open
//! document's directory to the filesystem root (or to the first file that
//! contains `root = true`), which is the standard EditorConfig discovery
//! algorithm. The walk intentionally follows the spec and may reach files
//! above the workspace root (e.g. `~/.editorconfig`, `/.editorconfig`).
//! Users who do not want those files to apply should add `root = true` to
//! their workspace `.editorconfig`.
//!
//! # Security accepted risks
//!
//! - **ReDoS via glob patterns**: Section headers in `.editorconfig` files
//!   are evaluated by `ec4rs`'s glob engine. A crafted file could use
//!   pathological patterns; this is accepted because the attacker who can
//!   write `.editorconfig` already has arbitrary filesystem write access.
//!   `ec4rs` has no published security advisories as of the last review.
//!
//! - **Unbounded cache growth**: The cache is keyed by directory and grows
//!   with each distinct directory visited. For typical single-project usage
//!   this is bounded by the project depth. `invalidate_all()` clears the
//!   entire cache on `.editorconfig` change events. An LRU cap was
//!   considered but deferred: local LSP sessions rarely exceed hundreds of
//!   distinct directories, and the added complexity is not warranted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ec4rs::property::{EndOfLine, FinalNewline, MaxLineLen};
use tower_lsp::lsp_types::Url;

/// Maximum accepted `max_line_length` value. Values above this are treated
/// as unset to avoid feeding an absurdly large width to the formatter.
const MAX_LINE_LENGTH_CAP: usize = 10_000;

/// Line-ending style from `.editorconfig`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix line feed (`\n`).
    Lf,
    /// Windows carriage-return + line feed (`\r\n`).
    Crlf,
    /// Classic Mac carriage return (`\r`).
    Cr,
}

/// Settings resolved from `.editorconfig` for a single document.
///
/// All fields are `None` when no `.editorconfig` is found, when the
/// document URI is not a `file:` URI, or when a parse error occurs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorConfigSettings {
    /// `max_line_length` from the matching section, capped at
    /// [`MAX_LINE_LENGTH_CAP`].
    pub max_line_length: Option<usize>,
    /// `end_of_line` from the matching section.
    pub end_of_line: Option<LineEnding>,
    /// `insert_final_newline` from the matching section.
    pub insert_final_newline: Option<bool>,
}

/// Module-level cache: directory → resolved settings.
///
/// Keyed by the directory containing the document, not the document path
/// itself, so all files in the same directory share one entry.
///
/// No guard is held across the `ec4rs::properties_of` call (see
/// `resolve` implementation); concurrent misses for the same directory are
/// resolved idempotently (last writer wins).
static CACHE: std::sync::LazyLock<Mutex<HashMap<PathBuf, EditorConfigSettings>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Resolve `.editorconfig` settings for the document at `uri`.
///
/// Returns empty settings if `uri` is not a `file:` URI, if no
/// `.editorconfig` is found in any ancestor directory, or if parsing fails.
///
/// Results are cached by directory. Call [`invalidate_all`] when
/// `.editorconfig` files change.
///
/// # Errors (handled internally, never propagated)
///
/// - Non-`file:` URI → empty settings
/// - No `.editorconfig` in tree → empty settings
/// - Malformed `.editorconfig` → empty settings (logged at debug level in
///   the future when a logger is wired in)
pub fn resolve(uri: &Url) -> EditorConfigSettings {
    let Ok(file_path) = uri.to_file_path() else {
        return EditorConfigSettings::default();
    };

    let Some(dir) = file_path.parent().map(Path::to_path_buf) else {
        return EditorConfigSettings::default();
    };

    // Check the cache without holding the lock across I/O.
    let cached = CACHE.lock().ok().and_then(|guard| guard.get(&dir).cloned());

    if let Some(settings) = cached {
        return settings;
    }

    // Cache miss — call ec4rs outside any lock.
    let settings = ec4rs::properties_of(&file_path).map_or_else(
        |_| EditorConfigSettings::default(),
        |props| settings_from_props(&props),
    );

    // Insert into cache; concurrent misses are idempotent (last writer wins).
    if let Ok(mut guard) = CACHE.lock() {
        guard.entry(dir).or_insert_with(|| settings.clone());
    }

    settings
}

/// Clear the entire cache.
///
/// Should be called when `.editorconfig` files change so that the next
/// `resolve` call re-reads from disk.
pub fn invalidate_all() {
    if let Ok(mut guard) = CACHE.lock() {
        guard.clear();
    }
}

fn settings_from_props(props: &ec4rs::Properties) -> EditorConfigSettings {
    let max_line_length = props.get::<MaxLineLen>().ok().and_then(|v| match v {
        MaxLineLen::Value(n) if n <= MAX_LINE_LENGTH_CAP => Some(n),
        MaxLineLen::Value(_) | MaxLineLen::Off => None,
    });

    let end_of_line = props.get::<EndOfLine>().ok().map(|v| match v {
        EndOfLine::Lf => LineEnding::Lf,
        EndOfLine::CrLf => LineEnding::Crlf,
        EndOfLine::Cr => LineEnding::Cr,
    });

    let insert_final_newline = props.get::<FinalNewline>().ok().map(|v| match v {
        FinalNewline::Value(b) => b,
    });

    EditorConfigSettings {
        max_line_length,
        end_of_line,
        insert_final_newline,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;
    use tower_lsp::lsp_types::Url;

    use super::{EditorConfigSettings, LineEnding, invalidate_all, resolve};

    fn file_uri(path: &Path) -> Url {
        Url::from_file_path(path).expect("valid file path")
    }

    fn write_editorconfig(dir: &Path, content: &str) {
        fs::write(dir.join(".editorconfig"), content).expect("write .editorconfig");
    }

    // Ensure the global cache does not bleed state between tests.
    fn clear() {
        invalidate_all();
    }

    // ---- 1: No .editorconfig present ----

    #[test]
    fn resolve_returns_empty_when_no_editorconfig_present() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        let result = resolve(&file_uri(&file));
        assert_eq!(result, EditorConfigSettings::default());
    }

    // ---- 2: max_line_length from [*.yaml] ----

    #[test]
    fn resolve_reads_max_line_length_from_yaml_section() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.yaml]\nmax_line_length = 100\n");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        let result = resolve(&file_uri(&file));
        assert_eq!(result.max_line_length, Some(100));
        assert_eq!(result.end_of_line, None);
        assert_eq!(result.insert_final_newline, None);
    }

    // ---- 3-7: end_of_line and insert_final_newline fields (rstest-style) ----

    #[test]
    fn resolve_reads_end_of_line_lf() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.yaml]\nend_of_line = lf\n");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).end_of_line, Some(LineEnding::Lf));
    }

    #[test]
    fn resolve_reads_end_of_line_crlf() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.yaml]\nend_of_line = crlf\n");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(
            resolve(&file_uri(&file)).end_of_line,
            Some(LineEnding::Crlf)
        );
    }

    #[test]
    fn resolve_reads_end_of_line_cr() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.yaml]\nend_of_line = cr\n");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).end_of_line, Some(LineEnding::Cr));
    }

    #[test]
    fn resolve_reads_insert_final_newline_true() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.yaml]\ninsert_final_newline = true\n");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).insert_final_newline, Some(true));
    }

    #[test]
    fn resolve_reads_insert_final_newline_false() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.yaml]\ninsert_final_newline = false\n");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).insert_final_newline, Some(false));
    }

    // ---- 8-9: [*.{yml,yaml}] glob applies to both extensions ----

    #[test]
    fn resolve_honors_yml_glob_for_yml_file() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.{yml,yaml}]\nmax_line_length = 120\n");
        let file = dir.path().join("config.yml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).max_line_length, Some(120));
    }

    #[test]
    fn resolve_honors_yml_glob_for_yaml_file() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.{yml,yaml}]\nmax_line_length = 120\n");
        let file = dir.path().join("config.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).max_line_length, Some(120));
    }

    // ---- 10: Walk up multiple directories ----

    #[test]
    fn resolve_walks_up_to_parent_directory() {
        let root = TempDir::new().unwrap();
        write_editorconfig(root.path(), "[*.yaml]\nmax_line_length = 80\n");
        let nested = root.path().join("subdir").join("nested");
        fs::create_dir_all(&nested).unwrap();
        let file = nested.join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).max_line_length, Some(80));
    }

    // ---- 11: root = true terminates walk ----

    #[test]
    fn resolve_root_true_terminates_walk() {
        let root = TempDir::new().unwrap();
        write_editorconfig(root.path(), "[*.yaml]\nmax_line_length = 80\n");
        let inner = root.path().join("project");
        fs::create_dir_all(&inner).unwrap();
        write_editorconfig(&inner, "root = true\n[*.yaml]\nmax_line_length = 100\n");
        let file = inner.join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).max_line_length, Some(100));
    }

    // ---- 12: Later section overrides earlier in same file ----

    #[test]
    fn resolve_later_section_overrides_earlier_in_same_file() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(
            dir.path(),
            "[*]\nmax_line_length = 80\n[*.yaml]\nmax_line_length = 100\n",
        );
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        assert_eq!(resolve(&file_uri(&file)).max_line_length, Some(100));
    }

    // ---- 13: indent_style = tab is silently dropped ----

    #[test]
    fn resolve_silently_drops_indent_style_tab() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(
            dir.path(),
            "[*.yaml]\nindent_style = tab\nmax_line_length = 90\n",
        );
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        let result = resolve(&file_uri(&file));
        // No panic, and the other fields still resolve correctly.
        assert_eq!(result.max_line_length, Some(90));
    }

    // ---- 14: Malformed .editorconfig ----

    #[test]
    fn resolve_returns_empty_for_malformed_editorconfig() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "this is not valid editorconfig syntax!!!");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        let result = resolve(&file_uri(&file));
        assert_eq!(result, EditorConfigSettings::default());
    }

    // ---- 15-16: Non-file URIs ----

    #[test]
    fn resolve_returns_empty_for_non_file_uri_untitled() {
        let uri = Url::parse("untitled:Untitled-1").expect("valid URI");
        let result = resolve(&uri);
        assert_eq!(result, EditorConfigSettings::default());
    }

    #[test]
    fn resolve_returns_empty_for_non_file_uri_inmemory() {
        let uri = Url::parse("inmemory://model/1").expect("valid URI");
        let result = resolve(&uri);
        assert_eq!(result, EditorConfigSettings::default());
    }

    // ---- 17: Cache hit returns consistent result ----

    #[test]
    fn cache_hit_returns_same_result_on_second_call() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.yaml]\nmax_line_length = 100\n");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        let first = resolve(&file_uri(&file));
        let second = resolve(&file_uri(&file));
        assert_eq!(first.max_line_length, Some(100));
        assert_eq!(second.max_line_length, Some(100));
    }

    // ---- 18: invalidate_all clears cache ----

    #[test]
    fn invalidate_all_clears_cache() {
        let dir = TempDir::new().unwrap();
        let ec_path = dir.path().join(".editorconfig");
        fs::write(&ec_path, "[*.yaml]\nmax_line_length = 100\n").unwrap();
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        let first = resolve(&file_uri(&file));
        assert_eq!(first.max_line_length, Some(100));
        invalidate_all();
        fs::remove_file(&ec_path).unwrap();
        let second = resolve(&file_uri(&file));
        assert_eq!(second, EditorConfigSettings::default());
    }

    // ---- 19: Default struct is all-None ----

    #[test]
    fn default_settings_are_all_none() {
        let s = EditorConfigSettings::default();
        assert_eq!(s.max_line_length, None);
        assert_eq!(s.end_of_line, None);
        assert_eq!(s.insert_final_newline, None);
    }

    // ---- Security: max_line_length cap ----

    #[test]
    fn resolve_caps_enormous_max_line_length() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(dir.path(), "[*.yaml]\nmax_line_length = 9999999999\n");
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        let result = resolve(&file_uri(&file));
        assert_eq!(result.max_line_length, None);
    }

    // ---- Security: pathological glob pattern (smoke test, no hang) ----

    #[test]
    fn resolve_handles_pathological_glob_without_hang() {
        let dir = TempDir::new().unwrap();
        write_editorconfig(
            dir.path(),
            "[{a,b,c,d,e,f,g,h,i,j}/*.{yaml,yml,json,toml}]\nmax_line_length = 80\n[*.yaml]\nmax_line_length = 90\n",
        );
        let file = dir.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        let result = resolve(&file_uri(&file));
        // [*.yaml] section applies; just verify it returns without hanging.
        assert_eq!(result.max_line_length, Some(90));
    }

    // ---- Security: symlink to .editorconfig outside workspace ----

    #[cfg(unix)]
    #[test]
    fn resolve_follows_symlinked_editorconfig() {
        use std::os::unix::fs::symlink;

        let outer = TempDir::new().unwrap();
        let ec_real = outer.path().join(".editorconfig");
        fs::write(&ec_real, "[*.yaml]\nmax_line_length = 77\n").unwrap();

        let inner = TempDir::new().unwrap();
        let ec_link = inner.path().join(".editorconfig");
        symlink(&ec_real, &ec_link).unwrap();

        let file = inner.path().join("file.yaml");
        fs::write(&file, "").unwrap();
        clear();
        // Expected behavior: symlink is followed; settings from the real file apply.
        let result = resolve(&file_uri(&file));
        assert_eq!(result.max_line_length, Some(77));
    }
}
