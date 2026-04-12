// SPDX-License-Identifier: MIT

/// The encoding detected from the input byte stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

/// Error produced when `decode` cannot convert the byte stream to UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodingError {
    /// Input bytes are not valid for the detected encoding.
    InvalidBytes,
    /// A UTF-16 or UTF-32 sequence contains an unpaired surrogate or an
    /// out-of-range codepoint.
    InvalidCodepoint(u32),
    /// UTF-16 input has an odd number of bytes.
    TruncatedUtf16,
    /// UTF-32 input length is not a multiple of four.
    TruncatedUtf32,
}

impl core::fmt::Display for EncodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidBytes => write!(f, "invalid byte sequence for detected encoding"),
            Self::InvalidCodepoint(cp) => write!(f, "invalid Unicode codepoint U+{cp:04X}"),
            Self::TruncatedUtf16 => write!(f, "UTF-16 stream has an odd number of bytes"),
            Self::TruncatedUtf32 => {
                write!(f, "UTF-32 stream length is not a multiple of four")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding detection
// ---------------------------------------------------------------------------

/// Detect the encoding of a YAML byte stream via BOM or null-byte heuristic.
///
/// Implements YAML 1.2 §5.2 encoding detection. UTF-32 BOMs are checked
/// before UTF-16 because the UTF-32 LE BOM (`FF FE 00 00`) is a superset of
/// the UTF-16 LE BOM (`FF FE`).
#[must_use]
pub fn detect_encoding(bytes: &[u8]) -> Encoding {
    match bytes {
        // UTF-32 BOMs (must come before UTF-16 checks)
        [0x00, 0x00, 0xFE, 0xFF, ..] => Encoding::Utf32Be,
        [0xFF, 0xFE, 0x00, 0x00, ..] => Encoding::Utf32Le,
        // UTF-16 BOMs
        [0xFE, 0xFF, ..] => Encoding::Utf16Be,
        [0xFF, 0xFE, ..] => Encoding::Utf16Le,
        // Null-byte heuristic (no BOM): YAML streams begin with ASCII content.
        // UTF-16 LE: odd bytes are null  → first pair is [<ascii>, 0x00]
        // UTF-16 BE: even bytes are null → first pair is [0x00, <ascii>]
        [a, 0x00, b, 0x00, ..] if *a != 0 && *b != 0 => Encoding::Utf16Le,
        [0x00, a, 0x00, b, ..] if *a != 0 && *b != 0 => Encoding::Utf16Be,
        [a, 0x00, ..] if *a != 0 => Encoding::Utf16Le,
        [0x00, a, ..] if *a != 0 => Encoding::Utf16Be,
        _ => Encoding::Utf8,
    }
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// Decode a YAML byte stream to a UTF-8 `String`, stripping any BOM.
///
/// Detects encoding via [`detect_encoding`], converts to UTF-8, and removes
/// the BOM character if present.
///
/// # Errors
///
/// Returns [`EncodingError`] if the byte stream is not valid for the detected
/// encoding, contains an invalid Unicode codepoint, or is truncated (odd-length
/// UTF-16 or non-multiple-of-four UTF-32).
pub fn decode(bytes: &[u8]) -> Result<String, EncodingError> {
    match detect_encoding(bytes) {
        Encoding::Utf8 => decode_utf8(bytes),
        Encoding::Utf16Le => decode_utf16(bytes, Endian::Little),
        Encoding::Utf16Be => decode_utf16(bytes, Endian::Big),
        Encoding::Utf32Le => decode_utf32(bytes, Endian::Little),
        Encoding::Utf32Be => decode_utf32(bytes, Endian::Big),
    }
}

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

fn decode_utf8(bytes: &[u8]) -> Result<String, EncodingError> {
    let s = core::str::from_utf8(bytes).map_err(|_| EncodingError::InvalidBytes)?;
    // Strip UTF-8 BOM (U+FEFF) if present.
    Ok(s.strip_prefix('\u{FEFF}').unwrap_or(s).to_owned())
}

fn decode_utf16(bytes: &[u8], endian: Endian) -> Result<String, EncodingError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(EncodingError::TruncatedUtf16);
    }
    // Collect u16 code units.
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| match (chunk, endian) {
            ([lo, hi], Endian::Little) => u16::from_le_bytes([*lo, *hi]),
            ([hi, lo], Endian::Big) => u16::from_be_bytes([*hi, *lo]),
            _ => 0, // chunks_exact(2) guarantees length 2; unreachable
        })
        .collect();

    // Strip BOM (U+FEFF) if first unit is the BOM codepoint.
    let units = match units.as_slice() {
        [0xFEFF, rest @ ..] => rest,
        other => other,
    };

    // Decode UTF-16 surrogate pairs.
    char::decode_utf16(units.iter().copied()).try_fold(
        String::with_capacity(units.len()),
        |mut s, r| match r {
            Ok(ch) => {
                s.push(ch);
                Ok(s)
            }
            Err(e) => Err(EncodingError::InvalidCodepoint(u32::from(
                e.unpaired_surrogate(),
            ))),
        },
    )
}

fn decode_utf32(bytes: &[u8], endian: Endian) -> Result<String, EncodingError> {
    if !bytes.len().is_multiple_of(4) {
        return Err(EncodingError::TruncatedUtf32);
    }
    let mut out = String::with_capacity(bytes.len() / 4);
    let mut skip_bom = true;
    for chunk in bytes.chunks_exact(4) {
        let cp = match (chunk, endian) {
            ([a, b, c, d], Endian::Little) => u32::from_le_bytes([*a, *b, *c, *d]),
            ([a, b, c, d], Endian::Big) => u32::from_be_bytes([*a, *b, *c, *d]),
            _ => 0, // chunks_exact(4) guarantees length 4; unreachable
        };
        // Strip leading BOM.
        if skip_bom && cp == 0xFEFF {
            skip_bom = false;
            continue;
        }
        skip_bom = false;
        let ch = char::from_u32(cp).ok_or(EncodingError::InvalidCodepoint(cp))?;
        out.push(ch);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Line-break normalization
// ---------------------------------------------------------------------------

/// Normalize all line breaks to LF (`\n`).
///
/// - `\r\n` (CRLF) → `\n`
/// - `\r` (lone CR) → `\n`
/// - `\n` (LF) — unchanged
#[must_use]
pub fn normalize_line_breaks(s: String) -> String {
    // Fast path: no CR means nothing to do.
    if !s.contains('\r') {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            // Consume the following LF of a CRLF pair so it is not doubled.
            if chars.peek() == Some(&'\n') {
                let _ = chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
mod tests {
    use rstest::rstest;

    use super::*;

    // -----------------------------------------------------------------------
    // detect_encoding
    // -----------------------------------------------------------------------

    #[test]
    fn detect_encoding_returns_utf8_for_empty_bytes() {
        assert_eq!(detect_encoding(b""), Encoding::Utf8);
    }

    #[rstest]
    #[case::utf8_bom(&[0xEF, 0xBB, 0xBF, b'a'], Encoding::Utf8)]
    #[case::utf16_le_bom(&[0xFF, 0xFE, b'a', 0x00], Encoding::Utf16Le)]
    #[case::utf16_be_bom(&[0xFE, 0xFF, 0x00, b'a'], Encoding::Utf16Be)]
    #[case::utf32_le_bom(&[0xFF, 0xFE, 0x00, 0x00], Encoding::Utf32Le)]
    #[case::utf32_be_bom(&[0x00, 0x00, 0xFE, 0xFF], Encoding::Utf32Be)]
    fn detect_encoding_bom(#[case] bytes: &[u8], #[case] expected: Encoding) {
        assert_eq!(detect_encoding(bytes), expected);
    }

    #[test]
    fn detect_encoding_falls_back_to_utf8_for_plain_ascii() {
        assert_eq!(detect_encoding(b"key: value\n"), Encoding::Utf8);
    }

    #[rstest]
    #[case::utf16_le_without_bom(&[b'a', 0x00, b'b', 0x00], Encoding::Utf16Le)]
    #[case::utf16_be_without_bom(&[0x00, b'a', 0x00, b'b'], Encoding::Utf16Be)]
    fn detect_encoding_null_byte_heuristic(#[case] bytes: &[u8], #[case] expected: Encoding) {
        assert_eq!(detect_encoding(bytes), expected);
    }

    // -----------------------------------------------------------------------
    // decode
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::utf8_plain_ascii(b"hello: world\n" as &[u8], "hello: world\n")]
    #[case::utf8_strips_bom(&[0xEF, 0xBB, 0xBF, b'k', b'e', b'y'], "key")]
    #[case::utf16_le_no_bom(&[0x68, 0x00, 0x69, 0x00], "hi")]
    #[case::utf16_be_no_bom(&[0x00, 0x68, 0x00, 0x69], "hi")]
    #[case::utf16_le_strips_bom(&[0xFF, 0xFE, 0x68, 0x00, 0x69, 0x00], "hi")]
    #[case::empty_input(b"", "")]
    fn decode_ok(#[case] bytes: &[u8], #[case] expected: &str) {
        assert_eq!(decode(bytes).unwrap(), expected);
    }

    #[test]
    fn decode_invalid_utf8_returns_error() {
        // Lone continuation byte — not valid UTF-8, no BOM so treated as UTF-8
        assert!(decode(&[0x80]).is_err());
    }

    // -----------------------------------------------------------------------
    // normalize_line_breaks
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::crlf_to_lf("a\r\nb\r\nc".to_string(), "a\nb\nc")]
    #[case::lone_cr_to_lf("a\rb\rc".to_string(), "a\nb\nc")]
    #[case::lf_only_unchanged("a\nb\nc".to_string(), "a\nb\nc")]
    #[case::mixed_line_endings("a\r\nb\rc\nd".to_string(), "a\nb\nc\nd")]
    #[case::empty_string_unchanged(String::new(), "")]
    #[case::crlf_not_doubled("\r\n".to_string(), "\n")]
    fn normalize_line_breaks_cases(#[case] input: String, #[case] expected: &str) {
        assert_eq!(normalize_line_breaks(input), expected);
    }
}
