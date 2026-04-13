// SPDX-License-Identifier: MIT

use tower_lsp::lsp_types::{Color, ColorPresentation, Position, Range};

/// A color found at a specific range in the document.
pub struct ColorMatch {
    pub range: Range,
    pub color: Color,
}

// ──────────────────────────────────────────────────────────────────────────────
// CSS named colors (alphabetically sorted for binary search)
// ──────────────────────────────────────────────────────────────────────────────

static NAMED_COLORS: &[(&str, [u8; 3])] = &[
    ("aliceblue", [240, 248, 255]),
    ("antiquewhite", [250, 235, 215]),
    ("aqua", [0, 255, 255]),
    ("aquamarine", [127, 255, 212]),
    ("azure", [240, 255, 255]),
    ("beige", [245, 245, 220]),
    ("bisque", [255, 228, 196]),
    ("black", [0, 0, 0]),
    ("blanchedalmond", [255, 235, 205]),
    ("blue", [0, 0, 255]),
    ("blueviolet", [138, 43, 226]),
    ("brown", [165, 42, 42]),
    ("burlywood", [222, 184, 135]),
    ("cadetblue", [95, 158, 160]),
    ("chartreuse", [127, 255, 0]),
    ("chocolate", [210, 105, 30]),
    ("coral", [255, 127, 80]),
    ("cornflowerblue", [100, 149, 237]),
    ("cornsilk", [255, 248, 220]),
    ("crimson", [220, 20, 60]),
    ("cyan", [0, 255, 255]),
    ("darkblue", [0, 0, 139]),
    ("darkcyan", [0, 139, 139]),
    ("darkgoldenrod", [184, 134, 11]),
    ("darkgray", [169, 169, 169]),
    ("darkgreen", [0, 100, 0]),
    ("darkgrey", [169, 169, 169]),
    ("darkkhaki", [189, 183, 107]),
    ("darkmagenta", [139, 0, 139]),
    ("darkolivegreen", [85, 107, 47]),
    ("darkorange", [255, 140, 0]),
    ("darkorchid", [153, 50, 204]),
    ("darkred", [139, 0, 0]),
    ("darksalmon", [233, 150, 122]),
    ("darkseagreen", [143, 188, 143]),
    ("darkslateblue", [72, 61, 139]),
    ("darkslategray", [47, 79, 79]),
    ("darkslategrey", [47, 79, 79]),
    ("darkturquoise", [0, 206, 209]),
    ("darkviolet", [148, 0, 211]),
    ("deeppink", [255, 20, 147]),
    ("deepskyblue", [0, 191, 255]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("dodgerblue", [30, 144, 255]),
    ("firebrick", [178, 34, 34]),
    ("floralwhite", [255, 250, 240]),
    ("forestgreen", [34, 139, 34]),
    ("fuchsia", [255, 0, 255]),
    ("gainsboro", [220, 220, 220]),
    ("ghostwhite", [248, 248, 255]),
    ("gold", [255, 215, 0]),
    ("goldenrod", [218, 165, 32]),
    ("gray", [128, 128, 128]),
    ("green", [0, 128, 0]),
    ("greenyellow", [173, 255, 47]),
    ("grey", [128, 128, 128]),
    ("honeydew", [240, 255, 240]),
    ("hotpink", [255, 105, 180]),
    ("indianred", [205, 92, 92]),
    ("indigo", [75, 0, 130]),
    ("ivory", [255, 255, 240]),
    ("khaki", [240, 230, 140]),
    ("lavender", [230, 230, 250]),
    ("lavenderblush", [255, 240, 245]),
    ("lawngreen", [124, 252, 0]),
    ("lemonchiffon", [255, 250, 205]),
    ("lightblue", [173, 216, 230]),
    ("lightcoral", [240, 128, 128]),
    ("lightcyan", [224, 255, 255]),
    ("lightgoldenrodyellow", [250, 250, 210]),
    ("lightgray", [211, 211, 211]),
    ("lightgreen", [144, 238, 144]),
    ("lightgrey", [211, 211, 211]),
    ("lightpink", [255, 182, 193]),
    ("lightsalmon", [255, 160, 122]),
    ("lightseagreen", [32, 178, 170]),
    ("lightskyblue", [135, 206, 250]),
    ("lightslategray", [119, 136, 153]),
    ("lightslategrey", [119, 136, 153]),
    ("lightsteelblue", [176, 196, 222]),
    ("lightyellow", [255, 255, 224]),
    ("lime", [0, 255, 0]),
    ("limegreen", [50, 205, 50]),
    ("linen", [250, 240, 230]),
    ("magenta", [255, 0, 255]),
    ("maroon", [128, 0, 0]),
    ("mediumaquamarine", [102, 205, 170]),
    ("mediumblue", [0, 0, 205]),
    ("mediumorchid", [186, 85, 211]),
    ("mediumpurple", [147, 112, 219]),
    ("mediumseagreen", [60, 179, 113]),
    ("mediumslateblue", [123, 104, 238]),
    ("mediumspringgreen", [0, 250, 154]),
    ("mediumturquoise", [72, 209, 204]),
    ("mediumvioletred", [199, 21, 133]),
    ("midnightblue", [25, 25, 112]),
    ("mintcream", [245, 255, 250]),
    ("mistyrose", [255, 228, 225]),
    ("moccasin", [255, 228, 181]),
    ("navajowhite", [255, 222, 173]),
    ("navy", [0, 0, 128]),
    ("oldlace", [253, 245, 230]),
    ("olive", [128, 128, 0]),
    ("olivedrab", [107, 142, 35]),
    ("orange", [255, 165, 0]),
    ("orangered", [255, 69, 0]),
    ("orchid", [218, 112, 214]),
    ("palegoldenrod", [238, 232, 170]),
    ("palegreen", [152, 251, 152]),
    ("paleturquoise", [175, 238, 238]),
    ("palevioletred", [219, 112, 147]),
    ("papayawhip", [255, 239, 213]),
    ("peachpuff", [255, 218, 185]),
    ("peru", [205, 133, 63]),
    ("pink", [255, 192, 203]),
    ("plum", [221, 160, 221]),
    ("powderblue", [176, 224, 230]),
    ("purple", [128, 0, 128]),
    ("rebeccapurple", [102, 51, 153]),
    ("red", [255, 0, 0]),
    ("rosybrown", [188, 143, 143]),
    ("royalblue", [65, 105, 225]),
    ("saddlebrown", [139, 69, 19]),
    ("salmon", [250, 128, 114]),
    ("sandybrown", [244, 164, 96]),
    ("seagreen", [46, 139, 87]),
    ("seashell", [255, 245, 238]),
    ("sienna", [160, 82, 45]),
    ("silver", [192, 192, 192]),
    ("skyblue", [135, 206, 235]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("slategrey", [112, 128, 144]),
    ("snow", [255, 250, 250]),
    ("springgreen", [0, 255, 127]),
    ("steelblue", [70, 130, 180]),
    ("tan", [210, 180, 140]),
    ("teal", [0, 128, 128]),
    ("thistle", [216, 191, 216]),
    ("tomato", [255, 99, 71]),
    ("turquoise", [64, 224, 208]),
    ("violet", [238, 130, 238]),
    ("wheat", [245, 222, 179]),
    ("white", [255, 255, 255]),
    ("whitesmoke", [245, 245, 245]),
    ("yellow", [255, 255, 0]),
    ("yellowgreen", [154, 205, 50]),
];

// ──────────────────────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Scan `text` for color values in YAML value positions and return all matches.
#[must_use]
pub fn find_colors(text: &str) -> Vec<ColorMatch> {
    let mut results = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        // Skip comment-only lines
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') && !looks_like_hex_comment(trimmed) {
            continue;
        }

        // Determine the value portion: after first unquoted ':'
        let value_start = value_start_offset(line);
        let scan_str = &line[value_start..];
        let col_offset = value_start;

        scan_line_for_colors(scan_str, line_idx, col_offset, &mut results);
    }

    results
}

/// Convert a `Color` to its possible text representations.
#[must_use]
pub fn color_presentations(color: Color) -> Vec<ColorPresentation> {
    let red = color.red;
    let green = color.green;
    let blue = color.blue;
    let alpha = color.alpha;

    let opaque = (alpha - 1.0_f32).abs() < f32::EPSILON;
    let ru = channel_to_u8(red);
    let gu = channel_to_u8(green);
    let bu = channel_to_u8(blue);
    let au = channel_to_u8(alpha);

    let hex = if opaque {
        format!("#{ru:02x}{gu:02x}{bu:02x}")
    } else {
        format!("#{ru:02x}{gu:02x}{bu:02x}{au:02x}")
    };

    let rgb = if opaque {
        format!("rgb({ru}, {gu}, {bu})")
    } else {
        format!("rgba({ru}, {gu}, {bu}, {alpha:.2})")
    };

    let (hue, sat, lum) = rgb_to_hsl(red, green, blue);
    let hu = hsl_to_u32(hue);
    let su = hsl_to_u32(sat);
    let lu = hsl_to_u32(lum);
    let hsl = if opaque {
        format!("hsl({hu}, {su}%, {lu}%)")
    } else {
        format!("hsla({hu}, {su}%, {lu}%, {alpha:.2})")
    };

    vec![
        ColorPresentation {
            label: hex,
            ..ColorPresentation::default()
        },
        ColorPresentation {
            label: rgb,
            ..ColorPresentation::default()
        },
        ColorPresentation {
            label: hsl,
            ..ColorPresentation::default()
        },
    ]
}

/// Convert a 0.0–1.0 color channel to a 0–255 byte value.
fn channel_to_u8(v: f32) -> u8 {
    // Value is clamped to [0.0, 1.0] before multiplying, so result is [0.0, 255.0].
    // Saturating u8 cast via intermediate i32 to avoid sign-loss and truncation lints.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is clamped to [0,255] before cast; result always fits"
    )]
    let byte = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    byte
}

/// Convert an HSL component (hue 0–360, sat/lum 0–100) to a rounded integer.
fn hsl_to_u32(v: f32) -> u32 {
    // v is always non-negative (hue 0-360, sat/lum 0-100), round gives integer in range.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is non-negative and bounded; result always fits"
    )]
    let n = v.max(0.0).round() as u32;
    n
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Return true if a `#`-prefixed token looks like a hex color (3, 6, or 8 hex digits)
/// rather than a YAML comment line.
fn looks_like_hex_comment(trimmed: &str) -> bool {
    let rest = &trimmed[1..]; // skip the leading '#'
    let hex_len = rest.chars().take_while(char::is_ascii_hexdigit).count();
    // It's a hex color candidate if immediately after '#' come 3, 6, or 8 hex digits
    // followed by a non-hex character (or end of string).
    matches!(hex_len, 3 | 6 | 8)
        && rest
            .chars()
            .nth(hex_len)
            .is_none_or(|c| !c.is_ascii_hexdigit() && !c.is_alphanumeric())
}

/// Return the byte offset in `line` where the value portion begins.
/// For mapping lines (`key: value`), this is after the first `: `.
/// For other lines (sequence items, bare scalars), it is 0.
fn value_start_offset(line: &str) -> usize {
    // Find first ':' not inside quotes followed by space (or end of line)
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b':' if !in_single && !in_double => {
                let next = bytes.get(i + 1).copied();
                if next == Some(b' ') || next == Some(b'\t') || next.is_none() {
                    return i + 1; // include the ':' itself, value starts after
                }
            }
            _ => {}
        }
    }
    0
}

/// Scan `scan_str` (a value portion of a YAML line) for color patterns.
/// `line_idx` and `col_offset` are used to produce absolute positions.
fn scan_line_for_colors(
    scan_str: &str,
    line_idx: usize,
    col_offset: usize,
    results: &mut Vec<ColorMatch>,
) {
    let mut i = 0;
    let chars: Vec<char> = scan_str.chars().collect();

    while i < chars.len() {
        let ch = chars.get(i).copied().unwrap_or('\0');

        // Try hex color: #RGB, #RRGGBB, #RRGGBBAA
        if ch == '#' {
            if let Some((color, len)) = try_hex(chars.get(i + 1..).unwrap_or(&[])) {
                let start_col = col_offset + i;
                let end_col = start_col + 1 + len;
                results.push(ColorMatch {
                    range: make_range(line_idx, start_col, line_idx, end_col),
                    color,
                });
                i += 1 + len;
                continue;
            }
        }

        // Try function-form: rgb/rgba/hsl/hsla
        if let Some((color, len)) = try_color_function(chars.get(i..).unwrap_or(&[])) {
            let start_col = col_offset + i;
            let end_col = start_col + len;
            results.push(ColorMatch {
                range: make_range(line_idx, start_col, line_idx, end_col),
                color,
            });
            i += len;
            continue;
        }

        // Try named color (only at word boundary)
        let prev = i.checked_sub(1).and_then(|p| chars.get(p).copied());
        let at_word_start = prev.is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if at_word_start && ch.is_ascii_alphabetic() {
            if let Some((color, len)) = try_named_color(chars.get(i..).unwrap_or(&[])) {
                let start_col = col_offset + i;
                let end_col = start_col + len;
                results.push(ColorMatch {
                    range: make_range(line_idx, start_col, line_idx, end_col),
                    color,
                });
                i += len;
                continue;
            }
        }

        i += 1;
    }
}

/// Attempt to parse a hex color starting after the `#`.
/// Returns `(Color, digits_consumed)` or `None`.
fn try_hex(chars: &[char]) -> Option<(Color, usize)> {
    let hex_len = chars.iter().take_while(|c| c.is_ascii_hexdigit()).count();
    if !matches!(hex_len, 3 | 6 | 8) {
        return None;
    }
    // Must be followed by non-hex, non-alpha character (word boundary)
    if chars
        .get(hex_len)
        .is_some_and(|c| c.is_ascii_hexdigit() || c.is_alphabetic())
    {
        return None;
    }

    let hex_str: String = chars.get(..hex_len)?.iter().collect();
    let color = parse_hex_color(&hex_str)?;
    Some((color, hex_len))
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    match hex.len() {
        3 => {
            let red = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let green = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let blue = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            Some(Color {
                red: f32::from(red) / 255.0,
                green: f32::from(green) / 255.0,
                blue: f32::from(blue) / 255.0,
                alpha: 1.0,
            })
        }
        6 => {
            let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color {
                red: f32::from(red) / 255.0,
                green: f32::from(green) / 255.0,
                blue: f32::from(blue) / 255.0,
                alpha: 1.0,
            })
        }
        8 => {
            let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let alpha = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color {
                red: f32::from(red) / 255.0,
                green: f32::from(green) / 255.0,
                blue: f32::from(blue) / 255.0,
                alpha: f32::from(alpha) / 255.0,
            })
        }
        _ => None,
    }
}

/// Attempt to parse a CSS color function: `rgb(...)`, `rgba(...)`, `hsl(...)`, `hsla(...)`.
/// Returns `(Color, chars_consumed)` or `None`.
fn try_color_function(chars: &[char]) -> Option<(Color, usize)> {
    let s: String = chars.iter().collect();
    let lower = s.to_lowercase();

    let prefix = if lower.starts_with("rgba(") {
        "rgba("
    } else if lower.starts_with("rgb(") {
        "rgb("
    } else if lower.starts_with("hsla(") {
        "hsla("
    } else if lower.starts_with("hsl(") {
        "hsl("
    } else {
        return None;
    };

    let close = s[prefix.len()..].find(')')?;
    let args_str = &s[prefix.len()..prefix.len() + close];
    let total_len = prefix.len() + close + 1;

    let color = if prefix.starts_with("rgb") {
        parse_rgb_args(args_str, prefix.starts_with("rgba"))?
    } else {
        parse_hsl_args(args_str, prefix.starts_with("hsla"))?
    };

    Some((color, total_len))
}

fn parse_component(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        Some(v / 100.0)
    } else {
        let v: f32 = s.parse().ok()?;
        // integers 0-255 → 0.0-1.0
        Some(v / 255.0)
    }
}

fn parse_alpha_component(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        Some(v / 100.0)
    } else {
        s.parse().ok()
    }
}

fn parse_rgb_args(args: &str, has_alpha: bool) -> Option<Color> {
    let parts: Vec<&str> = args.split(',').collect();
    if has_alpha && parts.len() == 4 {
        let red = parse_component(parts.first()?)?.clamp(0.0, 1.0);
        let green = parse_component(parts.get(1)?)?.clamp(0.0, 1.0);
        let blue = parse_component(parts.get(2)?)?.clamp(0.0, 1.0);
        let alpha = parse_alpha_component(parts.get(3)?)?.clamp(0.0, 1.0);
        Some(Color {
            red,
            green,
            blue,
            alpha,
        })
    } else if !has_alpha && parts.len() == 3 {
        let red = parse_component(parts.first()?)?.clamp(0.0, 1.0);
        let green = parse_component(parts.get(1)?)?.clamp(0.0, 1.0);
        let blue = parse_component(parts.get(2)?)?.clamp(0.0, 1.0);
        Some(Color {
            red,
            green,
            blue,
            alpha: 1.0,
        })
    } else {
        None
    }
}

fn parse_hsl_args(args: &str, has_alpha: bool) -> Option<Color> {
    let parts: Vec<&str> = args.split(',').collect();
    let expected = if has_alpha { 4 } else { 3 };
    if parts.len() != expected {
        return None;
    }
    let hue: f32 = parts.first()?.trim().parse().ok()?;
    let sat_str = parts.get(1)?.trim().strip_suffix('%')?.trim();
    let lum_str = parts.get(2)?.trim().strip_suffix('%')?.trim();
    let sat: f32 = sat_str.parse::<f32>().ok()? / 100.0;
    let lum: f32 = lum_str.parse::<f32>().ok()? / 100.0;
    let alpha = if has_alpha {
        parse_alpha_component(parts.get(3)?)?.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let (red, green, blue) = hsl_to_rgb(hue / 360.0, sat, lum);
    Some(Color {
        red,
        green,
        blue,
        alpha,
    })
}

/// Attempt to match a CSS named color at the start of `chars`.
/// Returns `(Color, chars_consumed)` or `None`.
fn try_named_color(chars: &[char]) -> Option<(Color, usize)> {
    let word_len = chars.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    if word_len == 0 {
        return None;
    }
    // Must be at a word boundary after the name
    if chars
        .get(word_len)
        .is_some_and(|c| c.is_alphanumeric() || *c == '_')
    {
        return None;
    }

    let word: String = chars.get(..word_len)?.iter().collect();
    let lower = word.to_lowercase();

    let entry = NAMED_COLORS.binary_search_by_key(&lower.as_str(), |&(name, _)| name);
    if let Ok(idx) = entry {
        let [red, green, blue] = NAMED_COLORS.get(idx)?.1;
        return Some((
            Color {
                red: f32::from(red) / 255.0,
                green: f32::from(green) / 255.0,
                blue: f32::from(blue) / 255.0,
                alpha: 1.0,
            },
            word_len,
        ));
    }
    None
}

fn make_range(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Range {
    Range {
        start: Position {
            line: u32::try_from(start_line).unwrap_or(u32::MAX),
            character: u32::try_from(start_col).unwrap_or(u32::MAX),
        },
        end: Position {
            line: u32::try_from(end_line).unwrap_or(u32::MAX),
            character: u32::try_from(end_col).unwrap_or(u32::MAX),
        },
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Color math
// ──────────────────────────────────────────────────────────────────────────────

fn hsl_to_rgb(hue: f32, sat: f32, lum: f32) -> (f32, f32, f32) {
    if sat.abs() < f32::EPSILON {
        return (lum, lum, lum);
    }
    let q2 = if lum < 0.5 {
        lum * (1.0 + sat)
    } else {
        lum.mul_add(-sat, lum + sat)
    };
    let p2 = 2.0f32.mul_add(lum, -q2);
    (
        hue_to_rgb(p2, q2, hue + 1.0 / 3.0),
        hue_to_rgb(p2, q2, hue),
        hue_to_rgb(p2, q2, hue - 1.0 / 3.0),
    )
}

fn hue_to_rgb(p2: f32, q2: f32, mut t: f32) -> f32 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return ((q2 - p2) * 6.0).mul_add(t, p2);
    }
    if t < 0.5 {
        return q2;
    }
    if t < 2.0 / 3.0 {
        return ((q2 - p2) * (2.0 / 3.0 - t)).mul_add(6.0, p2);
    }
    p2
}

fn rgb_to_hsl(red: f32, green: f32, blue: f32) -> (f32, f32, f32) {
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lum = f32::midpoint(max, min);

    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, lum * 100.0);
    }

    let delta = max - min;
    let sat = if lum > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };

    let hue = if (max - red).abs() < f32::EPSILON {
        (green - blue) / delta + if green < blue { 6.0 } else { 0.0 }
    } else if (max - green).abs() < f32::EPSILON {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    };

    (hue / 6.0 * 360.0, sat * 100.0, lum * 100.0)
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "test code")]
mod tests {
    use rstest::rstest;

    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn color_eq(a: &Color, b: &Color) -> bool {
        approx_eq(a.red, b.red)
            && approx_eq(a.green, b.green)
            && approx_eq(a.blue, b.blue)
            && approx_eq(a.alpha, b.alpha)
    }

    // Group: find_colors_returns_empty — assert colors.is_empty()
    #[rstest]
    #[case::invalid_hex_gg("color: #gggggg")]
    #[case::unknown_named_color("color: notacolor")]
    #[case::comment_line_skipped("# this is a comment with red")]
    fn find_colors_returns_empty(#[case] text: &str) {
        let colors = find_colors(text);
        assert!(colors.is_empty(), "expected no colors for: {text:?}");
    }

    // ── Hex colors ──────────────────────────────────────────────────────────

    #[test]
    fn hex_3_digit_expands_correctly() {
        let colors = find_colors("color: #fff");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 1.0,
                green: 1.0,
                blue: 1.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn hex_6_digit_parses_correctly() {
        let colors = find_colors("color: #FF0000");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn hex_8_digit_parses_with_alpha() {
        let colors = find_colors("color: #00ff00ff");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 0.0,
                green: 1.0,
                blue: 0.0,
                alpha: 1.0
            }
        ));
    }

    // ── CSS named colors ────────────────────────────────────────────────────

    #[test]
    fn named_color_red_matched() {
        let colors = find_colors("color: red");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn named_color_aliceblue_matched() {
        let colors = find_colors("bg: aliceblue");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 240.0 / 255.0,
                green: 248.0 / 255.0,
                blue: 255.0 / 255.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn named_color_case_insensitive() {
        let colors = find_colors("color: RED");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0
            }
        ));
    }

    // ── RGB / RGBA ───────────────────────────────────────────────────────────

    #[test]
    fn rgb_integer_parses_correctly() {
        let colors = find_colors("color: rgb(255, 0, 0)");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn rgb_percentage_parses_correctly() {
        let colors = find_colors("color: rgb(100%, 0%, 0%)");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn rgba_with_alpha_parses_correctly() {
        let colors = find_colors("color: rgba(0, 0, 0, 0.5)");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 0.5
            }
        ));
    }

    // ── HSL / HSLA ───────────────────────────────────────────────────────────

    #[test]
    fn hsl_red_parses_to_red() {
        // hsl(0, 100%, 50%) = red
        let colors = find_colors("color: hsl(0, 100%, 50%)");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn hsla_semi_transparent_green() {
        // hsla(120, 100%, 50%, 0.5) = semi-transparent green
        let colors = find_colors("color: hsla(120, 100%, 50%, 0.5)");
        assert_eq!(colors.len(), 1);
        let c = &colors[0].color;
        assert!(approx_eq(c.red, 0.0));
        assert!(approx_eq(c.green, 1.0));
        assert!(approx_eq(c.blue, 0.0));
        assert!(approx_eq(c.alpha, 0.5));
    }

    // ── color_presentations roundtrip ────────────────────────────────────────

    #[test]
    fn color_presentations_opaque_red() {
        let color = Color {
            red: 1.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        };
        let presentations = color_presentations(color);
        assert_eq!(presentations.len(), 3);
        assert_eq!(presentations[0].label, "#ff0000");
        assert_eq!(presentations[1].label, "rgb(255, 0, 0)");
        assert_eq!(presentations[2].label, "hsl(0, 100%, 50%)");
    }

    #[test]
    fn color_presentations_with_alpha() {
        let color = Color {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.5,
        };
        let presentations = color_presentations(color);
        assert_eq!(presentations.len(), 3);
        assert!(presentations[0].label.starts_with("#000000"));
        assert!(presentations[1].label.starts_with("rgba("));
        assert!(presentations[2].label.starts_with("hsla("));
    }

    // ── Detection rules ──────────────────────────────────────────────────────

    #[test]
    fn color_in_value_detected_not_in_key() {
        // Key "red:" should not be detected; value "blue" should be
        let colors = find_colors("red: blue");
        assert_eq!(colors.len(), 1);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 0.0,
                green: 0.0,
                blue: 1.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn hex_color_in_value_detected_despite_hash() {
        // #hex in a value (not a comment line) should be detected
        let colors = find_colors("color: #ff0000");
        assert_eq!(colors.len(), 1);
    }

    #[test]
    fn range_positions_correct() {
        let colors = find_colors("color: red");
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].range.start.line, 0);
        // "color: " is 7 chars, so "red" starts at column 7
        assert_eq!(colors[0].range.start.character, 7);
        assert_eq!(colors[0].range.end.character, 10);
    }
}
