// SPDX-License-Identifier: MIT

use regex::RegexBuilder;

/// RFC 3339 full date-time: `YYYY-MM-DDTHH:MM:SS[.frac](Z|+HH:MM|-HH:MM)`.
pub(super) fn is_valid_date_time(s: &str) -> bool {
    // Split on 'T' or 't'
    let Some(t_pos) = s.find(['T', 't']) else {
        return false;
    };
    let (date_part, time_and_offset) = s.split_at(t_pos);
    let time_and_offset = &time_and_offset[1..]; // skip the 'T'
    is_valid_date(date_part) && is_valid_time(time_and_offset)
}

/// RFC 3339 full-date: `YYYY-MM-DD`.
pub(super) fn is_valid_date(s: &str) -> bool {
    // Length must be exactly 10: YYYY-MM-DD
    if s.len() != 10 {
        return false;
    }
    // Safety: length checked above; these indices are always in-bounds ASCII
    if s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-') {
        return false;
    }
    let Ok(year) = s[..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = s[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = s[8..10].parse::<u32>() else {
        return false;
    };
    if month == 0 || month > 12 || day == 0 {
        return false;
    }
    let max_day = days_in_month(year, month);
    day <= max_day
}

const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

const fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// RFC 3339 partial-time + time-offset: `HH:MM:SS[.frac](Z|+HH:MM|-HH:MM)`.
pub(super) fn is_valid_time(s: &str) -> bool {
    // Must end with Z/z or ±HH:MM
    let (time_part, offset_part) =
        if let Some(stripped) = s.strip_suffix('Z').or_else(|| s.strip_suffix('z')) {
            (stripped, "Z")
        } else {
            // Find offset sign from the end
            let Some(sign_pos) = s.rfind(['+', '-']) else {
                return false;
            };
            // sign_pos must be after the time (at least HH:MM:SS = 8 chars)
            if sign_pos < 8 {
                return false;
            }
            (&s[..sign_pos], &s[sign_pos..])
        };

    // Validate time_part: HH:MM:SS[.frac]
    let tb = time_part.as_bytes();
    if tb.len() < 8 {
        return false;
    }
    if tb.get(2) != Some(&b':') || tb.get(5) != Some(&b':') {
        return false;
    }
    let Ok(hour) = time_part[..2].parse::<u32>() else {
        return false;
    };
    let Ok(minute) = time_part[3..5].parse::<u32>() else {
        return false;
    };
    let Ok(second) = time_part[6..8].parse::<u32>() else {
        return false;
    };
    if hour > 23 || minute > 59 || second > 60 {
        // 60 is allowed for leap seconds
        return false;
    }
    // Optional fractional seconds
    if tb.len() > 8 {
        if tb.get(8) != Some(&b'.') {
            return false;
        }
        if time_part[9..].is_empty() || !time_part[9..].bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }

    // Validate offset
    if offset_part == "Z" {
        return true;
    }
    let offset = &offset_part[1..]; // skip sign
    if offset.len() != 5 || offset.as_bytes().get(2) != Some(&b':') {
        return false;
    }
    let Ok(off_h) = offset[..2].parse::<u32>() else {
        return false;
    };
    let Ok(off_m) = offset[3..5].parse::<u32>() else {
        return false;
    };
    off_h <= 23 && off_m <= 59
}

/// ISO 8601 duration: `P[nY][nM][nD][T[nH][nM][nS]]` or `PnW`.
pub(super) fn is_valid_duration(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('P') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    // Week form: PnW
    if let Some(w) = rest.strip_suffix('W') {
        return !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit());
    }
    // Split on 'T'
    let (date_part, time_part) = rest.find('T').map_or((rest, None), |t_pos| {
        (&rest[..t_pos], Some(&rest[t_pos + 1..]))
    });
    // Validate date designators: Y M D in order, each optional but non-repeating
    if !is_valid_duration_designators(date_part, &['Y', 'M', 'D']) {
        return false;
    }
    if let Some(tp) = time_part {
        if tp.is_empty() {
            return false; // 'T' present but nothing after it
        }
        if !is_valid_duration_designators(tp, &['H', 'M', 'S']) {
            return false;
        }
    }
    // At least one designator total
    !date_part.is_empty() || time_part.is_some_and(|t| !t.is_empty())
}

/// Validate that `s` is a sequence of `nX` tokens where X appears in `designators`
/// in order (no repeats, only forward).
fn is_valid_duration_designators(s: &str, designators: &[char]) -> bool {
    let mut remaining = s;
    let mut last_idx: Option<usize> = None;
    while !remaining.is_empty() {
        // Read digits
        let digit_end = remaining
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(remaining.len());
        if digit_end == 0 {
            return false; // designator without digits
        }
        if digit_end == remaining.len() {
            return false; // digits without designator at end
        }
        let designator = remaining.chars().nth(digit_end).unwrap_or('\0');
        let Some(idx) = designators.iter().position(|&d| d == designator) else {
            return false;
        };
        if let Some(prev) = last_idx {
            if idx <= prev {
                return false; // out of order or repeated
            }
        }
        last_idx = Some(idx);
        remaining = &remaining[digit_end + designator.len_utf8()..];
    }
    true
}

/// Very basic email validation: `local@domain` where domain contains at least one dot.
pub(super) fn is_valid_email(s: &str) -> bool {
    let Some(at_pos) = s.rfind('@') else {
        return false;
    };
    let local = &s[..at_pos];
    let domain = &s[at_pos + 1..];
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

/// IPv4: four decimal octets in `0-255` separated by dots.
pub(super) fn is_valid_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| {
        !p.is_empty()
            && p.len() <= 3
            && p.bytes().all(|b| b.is_ascii_digit())
            && p.parse::<u16>().is_ok_and(|n| n <= 255)
            && (p.len() == 1 || !p.starts_with('0')) // no leading zeros
    })
}

/// IPv6: eight groups of 1-4 hex digits separated by colons, with optional `::`.
pub(super) fn is_valid_ipv6(s: &str) -> bool {
    // Allow zone ID suffix (strip %...)
    let s = s.split('%').next().unwrap_or(s);
    // Handle embedded IPv4 in the last group
    let (s, ipv4_suffix) = if let Some(last_colon) = s.rfind(':') {
        let candidate = &s[last_colon + 1..];
        if candidate.contains('.') {
            if !is_valid_ipv4(candidate) {
                return false;
            }
            (&s[..last_colon], true)
        } else {
            (s, false)
        }
    } else {
        (s, false)
    };

    let has_double_colon = s.contains("::");
    // When splitting on "::", the halves may themselves be empty (e.g. "::1" → ["", "1"])
    // Filter those out before validating individual groups.
    let parts: Vec<&str> = if has_double_colon {
        s.splitn(2, "::")
            .flat_map(|h| h.split(':'))
            .filter(|p| !p.is_empty())
            .collect()
    } else {
        s.split(':').collect()
    };

    let expected = if ipv4_suffix { 6 } else { 8 };
    let max_groups = if has_double_colon {
        expected - 1
    } else {
        expected
    };

    if parts.len() > max_groups {
        return false;
    }
    if !has_double_colon && parts.len() != expected {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.len() <= 4 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Hostname per RFC 1123: labels of `[A-Za-z0-9-]`, each ≤63 chars, total ≤253.
pub(super) fn is_valid_hostname(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    // Strip optional trailing dot (FQDN)
    let s = s.strip_suffix('.').unwrap_or(s);
    s.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

/// URI: must have a scheme followed by `:`
pub(super) fn is_valid_uri(s: &str) -> bool {
    let Some(colon) = s.find(':') else {
        return false;
    };
    let scheme = &s[..colon];
    !scheme.is_empty()
        && scheme
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
}

/// URI-reference: either a valid URI or a relative reference (starts with `/`, `?`, `#`, or `//`).
pub(super) fn is_valid_uri_reference(s: &str) -> bool {
    if s.is_empty() {
        return true; // empty string is a valid URI-reference
    }
    is_valid_uri(s)
        || s.starts_with('/')
        || s.starts_with('?')
        || s.starts_with('#')
        || s.starts_with("//")
        || !s.contains(':') // relative-path reference
}

/// URI-template (RFC 6570): any printable ASCII string with balanced `{...}` expressions.
pub(super) fn is_valid_uri_template(s: &str) -> bool {
    let mut depth = 0u32;
    for b in s.bytes() {
        match b {
            b'{' => {
                if depth > 0 {
                    return false; // nested braces not allowed
                }
                depth += 1;
            }
            b'}' => {
                if depth == 0 {
                    return false; // unmatched closing brace
                }
                depth -= 1;
            }
            0x00..=0x1F | 0x7F => return false, // control chars
            _ => {}
        }
    }
    depth == 0
}

/// UUID: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` (case-insensitive).
pub(super) fn is_valid_uuid(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    // Check dashes at fixed positions (length already verified to be 36)
    if bytes.get(8) != Some(&b'-')
        || bytes.get(13) != Some(&b'-')
        || bytes.get(18) != Some(&b'-')
        || bytes.get(23) != Some(&b'-')
    {
        return false;
    }
    bytes.iter().enumerate().all(|(i, &b)| {
        if i == 8 || i == 13 || i == 18 || i == 23 {
            true // dash — already verified
        } else {
            b.is_ascii_hexdigit()
        }
    })
}

/// Validate a JSON Schema `regex` value by trying to compile it with the `regex` crate.
pub(super) fn is_valid_regex(s: &str) -> bool {
    if s.len() > super::MAX_PATTERN_LEN {
        return false;
    }
    RegexBuilder::new(s)
        .size_limit(super::REGEX_SIZE_LIMIT)
        .build()
        .is_ok()
}

/// JSON Pointer (RFC 6901): empty string or starts with `/`.
/// Each token may not contain unescaped `~` (must be `~0` or `~1`).
pub(super) fn is_valid_json_pointer(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if !s.starts_with('/') {
        return false;
    }
    is_json_pointer_tokens_valid(s)
}

fn is_json_pointer_tokens_valid(s: &str) -> bool {
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next() {
                Some('0' | '1') => {}
                _ => return false,
            }
        }
    }
    true
}

/// Relative JSON Pointer: non-negative integer followed by a JSON Pointer or `#`.
pub(super) fn is_valid_relative_json_pointer(s: &str) -> bool {
    let digit_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if digit_end == 0 {
        return false; // must start with a non-negative integer
    }
    let rest = &s[digit_end..];
    if rest == "#" {
        return true;
    }
    is_valid_json_pointer(rest)
}

/// IDN hostname: validates using IDNA UTS#46 strict processing (UseSTD3ASCIIRules=true).
pub(super) fn is_valid_idn_hostname(s: &str) -> bool {
    idna::domain_to_ascii_strict(s).is_ok()
}

/// IDN email: local@domain where domain is validated via IDNA strict processing.
pub(super) fn is_valid_idn_email(s: &str) -> bool {
    let Some(at_pos) = s.rfind('@') else {
        return false;
    };
    let local = &s[..at_pos];
    let domain = &s[at_pos + 1..];
    !local.is_empty() && idna::domain_to_ascii_strict(domain).is_ok()
}

/// IRI (Internationalized Resource Identifier, RFC 3987).
pub(super) fn is_valid_iri(s: &str) -> bool {
    iri_string::types::IriStr::new(s).is_ok()
}

/// IRI-reference (absolute IRI or relative reference, RFC 3987).
pub(super) fn is_valid_iri_reference(s: &str) -> bool {
    iri_string::types::IriReferenceStr::new(s).is_ok()
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "test code")]
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

    use crate::schema::{JsonSchema, SchemaType};
    use crate::schema_validation::validate_schema;
    use crate::server::YamlVersion;
    use rlsp_yaml_parser::Span;
    use rlsp_yaml_parser::node::Document;

    fn parse_docs(text: &str) -> Vec<Document<Span>> {
        rlsp_yaml_parser::load(text).unwrap_or_default()
    }

    fn code_of(d: &Diagnostic) -> &str {
        match &d.code {
            Some(NumberOrString::String(s)) => s.as_str(),
            _ => "",
        }
    }

    fn format_schema(fmt: &str) -> JsonSchema {
        JsonSchema {
            schema_type: Some(SchemaType::Single("string".to_string())),
            format: Some(fmt.to_string()),
            ..JsonSchema::default()
        }
    }

    fn run_format(text: &str, fmt: &str) -> Vec<Diagnostic> {
        let schema = format_schema(fmt);
        let docs = parse_docs(text);
        validate_schema(text, &docs, &schema, true, YamlVersion::V1_2)
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Format validation
    // ══════════════════════════════════════════════════════════════════════════

    // Tests 157, 159, 161, 163, 165, 167, 169, 171, 173, 175, 177, 179, 181, 183, 185:
    // valid format value → no diagnostics
    #[rstest]
    #[case::date_time_utc("2023-01-15T10:30:00Z", "date-time")]
    #[case::date_time_with_offset("2023-01-15T10:30:00+05:30", "date-time")]
    #[case::date_time_with_milliseconds("2023-01-15T10:30:00.123Z", "date-time")]
    #[case::date_simple("2023-01-15", "date")]
    #[case::date_leap_year("2024-02-29", "date")]
    #[case::email_simple("user@example.com", "email")]
    #[case::email_with_plus_and_subdomain("a+b@sub.domain.org", "email")]
    #[case::ipv4_typical("192.168.1.1", "ipv4")]
    #[case::ipv4_all_zeros("0.0.0.0", "ipv4")]
    #[case::ipv4_all_255("255.255.255.255", "ipv4")]
    #[case::ipv6_full("2001:0db8:85a3:0000:0000:8a2e:0370:7334", "ipv6")]
    #[case::ipv6_loopback("::1", "ipv6")]
    #[case::ipv6_link_local("fe80::1", "ipv6")]
    #[case::hostname_with_dots("example.com", "hostname")]
    #[case::hostname_subdomain("sub.example.com", "hostname")]
    #[case::hostname_single_label("localhost", "hostname")]
    #[case::uri_https("https://example.com/path", "uri")]
    #[case::uri_http("http://example.com", "uri")]
    #[case::uri_urn("urn:isbn:0451450523", "uri")]
    #[case::uuid_lowercase("550e8400-e29b-41d4-a716-446655440000", "uuid")]
    #[case::uuid_uppercase("550E8400-E29B-41D4-A716-446655440000", "uuid")]
    #[case::json_pointer_empty("", "json-pointer")]
    #[case::json_pointer_simple_path("/foo/bar", "json-pointer")]
    #[case::json_pointer_with_index("/foo/0", "json-pointer")]
    #[case::json_pointer_tilde0_escape("/a~0b", "json-pointer")]
    #[case::json_pointer_tilde1_escape("/a~1b", "json-pointer")]
    #[case::regex_anchored("^[a-z]+$", "regex")]
    #[case::regex_wildcard(".*", "regex")]
    #[case::idn_hostname_ascii("example.com", "idn-hostname")]
    #[case::idn_hostname_punycode("xn--nxasmq6b.com", "idn-hostname")]
    #[case::idn_hostname_subdomain("sub.example.org", "idn-hostname")]
    #[case::idn_email_ascii("user@example.com", "idn-email")]
    #[case::idn_email_punycode_domain("user@xn--nxasmq6b.com", "idn-email")]
    #[case::iri_https("https://example.com/path", "iri")]
    #[case::iri_http("http://example.com", "iri")]
    #[case::iri_urn("urn:isbn:0451450523", "iri")]
    #[case::iri_reference_absolute("https://example.com/path", "iri-reference")]
    #[case::iri_reference_root_relative("/relative/path", "iri-reference")]
    #[case::iri_reference_relative("relative/path", "iri-reference")]
    #[case::unknown_format_silently_ignored("anything", "some-unknown-format")]
    // duration — Group A valid cases
    #[case::duration_week_form("P3W", "duration")]
    #[case::duration_years_only("P1Y", "duration")]
    #[case::duration_months_only("P6M", "duration")]
    #[case::duration_date_and_time("P1Y2M3DT4H5M6S", "duration")]
    #[case::duration_time_only("PT30S", "duration")]
    #[case::duration_hours_minutes("PT2H30M", "duration")]
    #[case::duration_date_only_multi("P1Y2M10D", "duration")]
    // uri-template — Group B valid cases
    #[case::uri_template_simple("https://example.com/{path}", "uri-template")]
    #[case::uri_template_no_braces("https://example.com/path", "uri-template")]
    #[case::uri_template_multiple_expressions("{base}/{path}", "uri-template")]
    // relative-json-pointer — Group C valid cases
    // Note: "0#" and "100#" contain `#` which starts a YAML comment — use
    // quoted YAML via the direct-function tests below. "2/foo/bar" is safe.
    #[case::relative_json_pointer_multi_step_path("2/foo/bar", "relative-json-pointer")]
    // time — Group D valid cases (via date-time prefix for those with offsets)
    #[case::date_time_with_fractional_seconds("2023-01-15T12:30:00.999Z", "date-time")]
    #[case::date_time_with_positive_offset("2023-01-15T08:00:00+05:30", "date-time")]
    #[case::date_time_with_negative_offset("2023-01-15T20:00:00-08:00", "date-time")]
    #[case::date_time_leap_second("2023-06-30T23:59:60Z", "date-time")]
    // time format directly
    #[case::time_utc("12:30:00Z", "time")]
    #[case::time_with_offset("08:00:00+05:30", "time")]
    #[case::time_with_negative_offset("20:00:00-08:00", "time")]
    #[case::time_with_fractional_seconds("12:30:00.999Z", "time")]
    #[case::time_leap_second("23:59:60Z", "time")]
    // ipv6 — Group E valid cases
    #[case::ipv6_with_zone_id("fe80::1%eth0", "ipv6")]
    #[case::ipv6_ipv4_mapped("::ffff:192.168.1.1", "ipv6")]
    // ipv4 — Group F valid cases
    #[case::ipv4_single_digit_octets("1.2.3.4", "ipv4")]
    // hostname — Group G valid cases
    #[case::hostname_trailing_dot_fqdn("example.com.", "hostname")]
    // uri-reference — Group H valid cases
    #[case::uri_reference_starts_with_query("?query=1", "uri-reference")]
    #[case::uri_reference_starts_with_fragment("#section", "uri-reference")]
    #[case::uri_reference_double_slash("//authority/path", "uri-reference")]
    #[case::uri_reference_empty("", "uri-reference")]
    fn format_valid_produces_no_diagnostics(#[case] value: &str, #[case] fmt: &str) {
        assert!(run_format(value, fmt).is_empty());
    }

    // Tests 158, 160, 162, 164, 166, 168, 170, 172, 174, 178, 180, 182, 184, 186:
    // invalid format value → schemaFormat WARNING with format name in message
    #[rstest]
    #[case::date_time_not_a_date("not-a-date", "date-time")]
    #[case::date_time_invalid_month("2023-13-01T00:00:00Z", "date-time")]
    #[case::date_time_space_separator("2023-01-15 10:30:00Z", "date-time")]
    #[case::date_invalid_month("2023-13-01", "date")]
    #[case::date_non_leap_year_feb29("2023-02-29", "date")]
    #[case::date_not_a_date("not-a-date", "date")]
    #[case::email_no_at_sign("no-at-sign", "email")]
    #[case::email_missing_domain_dot("missing-domain-dot@nodot", "email")]
    #[case::email_no_domain("user-no-domain@", "email")]
    #[case::ipv4_octet_too_large("256.0.0.1", "ipv4")]
    #[case::ipv4_too_few_octets("192.168.1", "ipv4")]
    #[case::ipv4_too_many_octets("192.168.1.1.1", "ipv4")]
    #[case::ipv4_leading_zero("01.0.0.1", "ipv4")]
    #[case::ipv6_too_many_groups("not::an::ipv6::address::with::too::many::groups::here", "ipv6")]
    #[case::hostname_leading_hyphen("-invalid.com", "hostname")]
    #[case::hostname_trailing_hyphen_label("invalid-.com", "hostname")]
    #[case::hostname_double_dot("invalid..com", "hostname")]
    #[case::uri_no_scheme("not-a-uri", "uri")]
    #[case::uri_relative_reference("//no-scheme", "uri")]
    #[case::uuid_not_uuid("not-a-uuid", "uuid")]
    #[case::uuid_invalid_char("550e8400-e29b-41d4-a716-44665544000g", "uuid")]
    #[case::uuid_no_hyphens("550e8400e29b41d4a716446655440000", "uuid")]
    #[case::json_pointer_no_leading_slash("foo", "json-pointer")]
    #[case::json_pointer_invalid_escape("/foo~2bar", "json-pointer")]
    #[case::json_pointer_trailing_tilde("/foo~", "json-pointer")]
    #[case::regex_unclosed_paren("(unclosed-paren", "regex")]
    #[case::idn_hostname_with_space("not a hostname", "idn-hostname")]
    #[case::idn_hostname_leading_hyphen("-bad-start.com", "idn-hostname")]
    #[case::idn_email_no_at_sign("no-at-sign", "idn-email")]
    #[case::idn_email_bad_domain("user@-bad-domain.com", "idn-email")]
    #[case::iri_with_space("not an iri", "iri")]
    #[case::iri_missing_scheme("://missing-scheme", "iri")]
    #[case::iri_reference_with_space("not valid iri ref", "iri-reference")]
    // duration — Group A invalid cases
    #[case::duration_missing_p("1Y", "duration")]
    #[case::duration_empty("P", "duration")]
    #[case::duration_week_form_empty_digits("PW", "duration")]
    #[case::duration_t_with_nothing_after("P1YT", "duration")]
    #[case::duration_designators_out_of_order("P1M1Y", "duration")]
    #[case::duration_repeated_designator("P1Y1Y", "duration")]
    #[case::duration_designator_without_digits("PY1M", "duration")]
    #[case::duration_digits_without_designator("P123", "duration")]
    // uri-template — `extra}` (unmatched close brace) — Group B invalid case.
    // `{{nested}}` and `{unclosed` are tested directly below (YAML parsing issues).
    #[case::uri_template_unmatched_close("extra}", "uri-template")]
    // relative-json-pointer — Group C invalid cases
    // `/foo` and `0foo` are safe YAML plain scalars.
    // Empty string and `0` require direct testing (see below).
    #[case::relative_json_pointer_no_digit("/foo", "relative-json-pointer")]
    #[case::relative_json_pointer_bad_pointer_part("0foo", "relative-json-pointer")]
    // time — Group D invalid cases
    #[case::time_invalid_fractional_missing_digits("12:30:00.", "time")]
    #[case::time_invalid_offset_missing_colon("12:30:00+0530", "time")]
    // ipv6 — Group E invalid cases
    #[case::ipv6_ipv4_mapped_invalid_ipv4("::ffff:999.0.0.1", "ipv6")]
    // ipv4 — Group F invalid cases
    #[case::ipv4_empty_octet("192..1.1", "ipv4")]
    // hostname — Group G invalid cases
    #[case::hostname_double_dot_empty_label("a..b.com", "hostname")]
    fn format_invalid_produces_schemaformat_warning(#[case] value: &str, #[case] fmt: &str) {
        let result = run_format(value, fmt);
        assert_eq!(result.len(), 1);
        assert_eq!(code_of(&result[0]), "schemaFormat");
        assert_eq!(result[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(result[0].message.contains(fmt));
    }

    // uri-template — direct unit tests for cases that conflict with YAML parsing.
    // `{{nested}}` parses as a YAML flow mapping; `{unclosed` is unterminated.
    // Control chars are rejected by YAML before reaching format validation.
    #[test]
    fn uri_template_direct_invalid_cases() {
        assert!(
            !super::is_valid_uri_template("{{nested}}"),
            "nested braces invalid"
        );
        assert!(
            !super::is_valid_uri_template("{unclosed"),
            "unmatched open brace invalid"
        );
        assert!(
            !super::is_valid_uri_template("\x01"),
            "C0 control char invalid"
        );
        assert!(
            !super::is_valid_uri_template("\x1F"),
            "C0 control char invalid"
        );
        assert!(
            !super::is_valid_uri_template("\x7F"),
            "DEL control char invalid"
        );
    }

    // uri-template empty string — valid (no expressions, no control chars).
    #[test]
    fn uri_template_empty_is_valid() {
        assert!(super::is_valid_uri_template(""));
    }

    // relative-json-pointer — direct unit tests for values that YAML misparses.
    // "0#" — the `#` starts a YAML comment, so `parse_docs("0#")` yields empty doc.
    // "0"  — YAML type coercion: the schema validator may emit schemaType for integer.
    // "0#" and "100#" must be tested by calling is_valid_relative_json_pointer directly.
    #[test]
    fn relative_json_pointer_direct_valid_cases() {
        assert!(
            super::is_valid_relative_json_pointer("0#"),
            "zero steps + hash valid"
        );
        assert!(
            super::is_valid_relative_json_pointer("0"),
            "zero steps + empty pointer valid"
        );
        assert!(
            super::is_valid_relative_json_pointer("100#"),
            "100 steps + hash valid"
        );
    }

    // relative-json-pointer — empty string is invalid (must start with a digit).
    #[test]
    fn relative_json_pointer_empty_is_invalid() {
        assert!(
            !super::is_valid_relative_json_pointer(""),
            "empty string invalid"
        );
    }

    // Test 176 — format_validation disabled: no diagnostics emitted
    // Uses validate_schema(..., false) directly — tests the feature flag, not a format value.
    #[test]
    fn format_validation_disabled_produces_no_format_diagnostics() {
        let schema = format_schema("date");
        let docs = parse_docs("not-a-date");
        let result = validate_schema("not-a-date", &docs, &schema, false, YamlVersion::V1_2);
        assert!(result.is_empty());
    }
}
