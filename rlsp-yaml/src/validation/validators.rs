// SPDX-License-Identifier: MIT

/// Unused-anchor and unresolved-alias validator.
pub mod anchors;
/// Custom tag types and parser.
pub mod custom_tag;
/// Custom tag validator.
pub mod custom_tags_validation;
/// Duplicate mapping key validator.
pub mod duplicate_keys;
/// Flow style validator.
pub mod flow_style;
/// Map key ordering validator.
pub mod key_ordering;
/// YAML 1.1 compatibility validator.
pub mod yaml11_compat;

pub use anchors::validate_unused_anchors;
pub use custom_tag::{CustomTag, TagNodeType, parse_custom_tag};
pub use custom_tags_validation::validate_custom_tags;
pub use duplicate_keys::validate_duplicate_keys;
pub use flow_style::validate_flow_style;
pub use key_ordering::validate_key_ordering;
pub use yaml11_compat::validate_yaml11_compat;
