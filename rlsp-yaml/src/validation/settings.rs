// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::server::Settings;

/// Categories of diagnostics whose severity is user-configurable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCategory {
    /// Flow-style collections (`{...}`, `[...]`) flagged by `validate_flow_style`.
    FlowStyle,
    /// Duplicate mapping keys flagged by `validate_duplicate_keys`.
    DuplicateKey,
}

/// Resolved, typed view of validation-related settings.
///
/// Constructed once at the parse-and-publish boundary from raw `Settings`.
/// `None` means the category is disabled ("off"); `Some(severity)` means emit
/// diagnostics at that severity.
#[derive(Debug, Clone)]
pub struct ValidationSettings {
    /// Configured severity for flow-style diagnostics, or `None` if disabled.
    pub flow_style: Option<DiagnosticSeverity>,
    /// Configured severity for duplicate-key diagnostics, or `None` if disabled.
    pub duplicate_keys: Option<DiagnosticSeverity>,
}

impl Default for ValidationSettings {
    fn default() -> Self {
        Self {
            flow_style: Some(DiagnosticSeverity::WARNING),
            duplicate_keys: Some(DiagnosticSeverity::ERROR),
        }
    }
}

impl ValidationSettings {
    /// Return the configured severity for `category`, or `None` if disabled.
    #[must_use]
    pub const fn severity_for(&self, category: DiagnosticCategory) -> Option<DiagnosticSeverity> {
        match category {
            DiagnosticCategory::FlowStyle => self.flow_style,
            DiagnosticCategory::DuplicateKey => self.duplicate_keys,
        }
    }

    /// Parse raw `Settings` strings into a typed `ValidationSettings`.
    ///
    /// Unknown strings fall back to the default severity for each category.
    #[must_use]
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            flow_style: parse_severity(settings.flow_style.as_deref(), DiagnosticSeverity::WARNING),
            duplicate_keys: parse_severity(
                settings.duplicate_keys.as_deref(),
                DiagnosticSeverity::ERROR,
            ),
        }
    }
}

/// Parse a severity string to `Option<DiagnosticSeverity>`.
///
/// - `None` (absent) → `Some(default_severity)`
/// - `"off"` → `None`
/// - `"warning"` → `Some(WARNING)`
/// - `"error"` → `Some(ERROR)`
/// - unknown → `Some(default_severity)`
fn parse_severity(
    value: Option<&str>,
    default_severity: DiagnosticSeverity,
) -> Option<DiagnosticSeverity> {
    match value {
        Some("off") => None,
        Some("warning") => Some(DiagnosticSeverity::WARNING),
        Some("error") => Some(DiagnosticSeverity::ERROR),
        None | Some(_) => Some(default_severity),
    }
}

#[cfg(test)]
mod tests {
    use tower_lsp::lsp_types::DiagnosticSeverity;

    use super::*;
    use crate::server::Settings;

    fn settings_with_flow_style(value: Option<&str>) -> Settings {
        Settings {
            flow_style: value.map(str::to_owned),
            ..Settings::default()
        }
    }

    fn settings_with_duplicate_keys(value: Option<&str>) -> Settings {
        Settings {
            duplicate_keys: value.map(str::to_owned),
            ..Settings::default()
        }
    }

    #[test]
    fn from_settings_absent_flow_style_defaults_to_warning() {
        let s = settings_with_flow_style(None);
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.flow_style, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn from_settings_warning_string_maps_to_warning() {
        let s = settings_with_flow_style(Some("warning"));
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.flow_style, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn from_settings_error_string_maps_to_error() {
        let s = settings_with_flow_style(Some("error"));
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.flow_style, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn from_settings_off_string_maps_to_none() {
        let s = settings_with_flow_style(Some("off"));
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.flow_style, None);
    }

    #[test]
    fn from_settings_unknown_string_falls_back_to_warning() {
        let s = settings_with_flow_style(Some("verbose"));
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.flow_style, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn severity_for_flow_style_returns_configured_value() {
        let vs = ValidationSettings {
            flow_style: Some(DiagnosticSeverity::ERROR),
            duplicate_keys: None,
        };
        assert_eq!(
            vs.severity_for(DiagnosticCategory::FlowStyle),
            Some(DiagnosticSeverity::ERROR)
        );
    }

    #[test]
    fn severity_for_flow_style_returns_none_when_off() {
        let vs = ValidationSettings {
            flow_style: None,
            duplicate_keys: None,
        };
        assert_eq!(vs.severity_for(DiagnosticCategory::FlowStyle), None);
    }

    // ---- duplicate_keys boundary parser tests ----

    #[test]
    fn from_settings_absent_duplicate_keys_defaults_to_error() {
        let s = settings_with_duplicate_keys(None);
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.duplicate_keys, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn from_settings_duplicate_keys_warning_string_maps_to_warning() {
        let s = settings_with_duplicate_keys(Some("warning"));
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.duplicate_keys, Some(DiagnosticSeverity::WARNING));
    }

    #[test]
    fn from_settings_duplicate_keys_error_string_maps_to_error() {
        let s = settings_with_duplicate_keys(Some("error"));
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.duplicate_keys, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn from_settings_duplicate_keys_off_string_maps_to_none() {
        let s = settings_with_duplicate_keys(Some("off"));
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.duplicate_keys, None);
    }

    #[test]
    fn from_settings_unknown_string_falls_back_to_error() {
        let s = settings_with_duplicate_keys(Some("verbose"));
        let vs = ValidationSettings::from_settings(&s);
        assert_eq!(vs.duplicate_keys, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn severity_for_duplicate_key_returns_configured_value() {
        let vs = ValidationSettings {
            duplicate_keys: Some(DiagnosticSeverity::WARNING),
            flow_style: None,
        };
        assert_eq!(
            vs.severity_for(DiagnosticCategory::DuplicateKey),
            Some(DiagnosticSeverity::WARNING)
        );
    }

    #[test]
    fn severity_for_duplicate_key_returns_none_when_off() {
        let vs = ValidationSettings {
            duplicate_keys: None,
            flow_style: None,
        };
        assert_eq!(vs.severity_for(DiagnosticCategory::DuplicateKey), None);
    }

    #[test]
    fn severity_for_flow_style_unaffected_by_duplicate_key_field() {
        let vs = ValidationSettings {
            duplicate_keys: None,
            flow_style: Some(DiagnosticSeverity::WARNING),
        };
        assert_eq!(
            vs.severity_for(DiagnosticCategory::FlowStyle),
            Some(DiagnosticSeverity::WARNING)
        );
    }
}
