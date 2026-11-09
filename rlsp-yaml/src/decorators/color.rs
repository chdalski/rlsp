// SPDX-License-Identifier: MIT

use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{LineBuffer, LineIndex, ScalarStyle, Span};
use tower_lsp::lsp_types::{Color, ColorPresentation, Range};

use crate::lsp_util::span_to_lsp;

/// A color found at a specific range in the document.
pub struct ColorMatch {
    /// Source range of the color value in the document.
    pub range: Range,
    /// The parsed RGBA color value.
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

/// Scan `docs` for color values in YAML value positions and return all matches.
///
/// `text` is the source `docs` were loaded from. Each match is reported at its
/// place in `text`, because a client replaces that range when the user picks
/// another color.
///
/// Comments are not represented in the AST, so colors in comments are never
/// returned (deliberate behavior change from the text-scanning predecessor).
///
/// The source lines of each scalar are scanned one by one, so a token that
/// straddles a line break is skipped. A color that exists only through escapes
/// of a double-quoted scalar has no place in the source and is not reported.
#[must_use]
pub fn find_colors(docs: &[Document<Span>], text: &str) -> Vec<ColorMatch> {
    let mut results = Vec::new();
    for doc in docs {
        let idx = doc.line_index();
        walk_node_value(&doc.root, text, &mut results, idx);
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
const fn hsl_to_u32(v: f32) -> u32 {
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
// AST walk — value-position only
// ──────────────────────────────────────────────────────────────────────────────

/// Walk `node` as a VALUE position, collecting color matches.
fn walk_node_value(node: &Node<Span>, text: &str, results: &mut Vec<ColorMatch>, idx: &LineIndex) {
    match node {
        Node::Scalar { style, loc, .. } => {
            scan_scalar_for_colors(*style, *loc, text, results, idx);
        }
        Node::Mapping { entries, .. } => {
            for (_key, value) in entries {
                walk_node_value(value, text, results, idx);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                walk_node_value(item, text, results, idx);
            }
        }
        Node::Alias { .. } => {}
    }
}

/// The longest color function in characters. A function that is longer is not
/// reported; the limit keeps the search for its closing parenthesis, and with
/// it the scan of a long line, linear in the size of the line.
const MAX_COLOR_FUNCTION_LEN: usize = 128;

/// One character of a scalar's source, or one escape sequence that stands for
/// a character, with the byte offsets in the document it covers.
#[derive(Clone, Copy)]
struct Cell {
    start: u32,
    end: u32,
    ch: char,
}

/// Scan a scalar's source text for color patterns, one source line at a time:
/// a color never reaches over a line break.
///
/// A block scalar's span starts at its `|` or `>` header, whose line (with any
/// comment) is not content and is skipped. A span that does not fit `text`
/// skips the scalar: nothing is guessed about where its colors lie.
fn scan_scalar_for_colors(
    style: ScalarStyle,
    loc: Span,
    text: &str,
    results: &mut Vec<ColorMatch>,
    idx: &LineIndex,
) {
    let Some(source) = text.get(loc.start as usize..loc.end as usize) else {
        return;
    };
    let is_block = matches!(style, ScalarStyle::Literal(_) | ScalarStyle::Folded(_));
    let mut lines = ScalarLines {
        lines: LineBuffer::new(source),
        base: loc.start,
        escapes: style == ScalarStyle::DoubleQuoted,
        after_escaped_break: false,
    };
    if is_block {
        let _header = lines.lines.consume_next();
    }
    for cells in lines {
        scan_cells(&cells, results, idx);
    }
}

/// The lines of a scalar's source as cells. Source lines that an escaped line
/// break joins make up one line, as they read in the scalar's value.
struct ScalarLines<'a> {
    lines: LineBuffer<'a>,
    /// Offset of the scalar's first byte in the document.
    base: u32,
    /// Whether backslash escapes are in effect (double-quoted scalars).
    escapes: bool,
    /// Whether the previous line ended in an escaped line break.
    after_escaped_break: bool,
}

impl Iterator for ScalarLines<'_> {
    type Item = Vec<Cell>;

    fn next(&mut self) -> Option<Vec<Cell>> {
        let mut cells = self.source_line()?;
        while self.after_escaped_break {
            let Some(joined) = self.source_line() else {
                break;
            };
            cells.extend(joined);
        }
        Some(cells)
    }
}

impl ScalarLines<'_> {
    /// The cells of the next source line, escapes masked.
    fn source_line(&mut self) -> Option<Vec<Cell>> {
        let line = self.lines.consume_next()?;
        // Offsets within the scalar's source fit `u32` because its span does.
        let at =
            |offset: usize| u32::try_from(offset).map_or(u32::MAX, |o| self.base.saturating_add(o));
        let mut cells: Vec<Cell> = line
            .content
            .char_indices()
            .map(|(o, ch)| Cell {
                start: at(line.offset + o),
                end: at(line.offset + o + ch.len_utf8()),
                ch,
            })
            .collect();
        if self.after_escaped_break {
            // The leading blanks of the line after an escaped line break are
            // not content.
            let blanks = cells
                .iter()
                .take_while(|c| matches!(c.ch, ' ' | '\t'))
                .count();
            cells.drain(..blanks);
        }
        if self.escapes {
            self.after_escaped_break = mask_escapes(&mut cells);
        }
        Some(cells)
    }
}

/// Stands for an escape that decodes to a letter, digit, or `_`: a color token
/// never ends or starts next to it. It is no ASCII letter, hex digit, or `_`,
/// so it never becomes part of a token.
const WORD_PLACEHOLDER: char = 'é';

/// Stands for an escape that decodes to a character that is no word character.
const BOUNDARY_PLACEHOLDER: char = '\\';

/// Replace each escape sequence in the `cells` of a double-quoted scalar's line
/// by one placeholder cell that covers the sequence, so a color token is
/// matched against what the escapes decode to at its borders. An escaped line
/// break at the end of the line is dropped; returns whether there was one.
fn mask_escapes(cells: &mut Vec<Cell>) -> bool {
    let mut masked = Vec::with_capacity(cells.len());
    let mut i = 0;
    let mut escaped_break = false;
    while let Some(&cell) = cells.get(i) {
        if cell.ch != '\\' {
            masked.push(cell);
            i += 1;
            continue;
        }
        let Some(kind) = cells.get(i + 1).map(|c| c.ch) else {
            escaped_break = true;
            break;
        };
        let digits = match kind {
            'x' => 2,
            'u' => 4,
            'U' => 8,
            _ => 0,
        };
        let decoded = if digits == 0 {
            None
        } else {
            cells
                .get(i + 2..i + 2 + digits)
                .and_then(|hex| {
                    hex.iter().try_fold(0u32, |acc, c| {
                        c.ch.to_digit(16).map(|value| (acc << 4) | value)
                    })
                })
                .and_then(char::from_u32)
        };
        let (placeholder, length) = match decoded {
            Some(c) if c.is_alphanumeric() || c == '_' => (WORD_PLACEHOLDER, 2 + digits),
            Some(_) => (BOUNDARY_PLACEHOLDER, 2 + digits),
            // A simple escape decodes to no word character. A malformed
            // numeric escape is a load error and cannot reach here.
            None => (BOUNDARY_PLACEHOLDER, 2),
        };
        let last = cells.get(i + length - 1).unwrap_or(&cell);
        masked.push(Cell {
            start: cell.start,
            end: last.end,
            ch: placeholder,
        });
        i += length;
    }
    *cells = masked;
    escaped_break
}

// ──────────────────────────────────────────────────────────────────────────────
// Line-level scanner
// ──────────────────────────────────────────────────────────────────────────────

/// Scan the `cells` of one line of a scalar's value for color patterns. A
/// color that lies over more than one source line is not reported.
fn scan_cells(cells: &[Cell], results: &mut Vec<ColorMatch>, idx: &LineIndex) {
    let chars: Vec<char> = cells.iter().map(|c| c.ch).collect();
    let mut push_match = |from: usize, len: usize, color: Color| {
        if let (Some(start), Some(end)) = (
            cells.get(from).map(|c| c.start),
            cells.get(from + len - 1).map(|c| c.end),
        ) {
            let range = span_to_lsp(Span { start, end }, idx);
            if range.start.line == range.end.line {
                results.push(ColorMatch { range, color });
            }
        }
    };
    let mut i = 0;

    while i < chars.len() {
        let ch = chars.get(i).copied().unwrap_or('\0');

        // Try hex color: #RGB, #RRGGBB, #RRGGBBAA
        if ch == '#'
            && let Some((color, len)) = try_hex(chars.get(i + 1..).unwrap_or(&[]))
        {
            push_match(i, 1 + len, color);
            i += 1 + len;
            continue;
        }

        // Try function-form: rgb/rgba/hsl/hsla
        if let Some((color, len)) = try_color_function(chars.get(i..).unwrap_or(&[])) {
            push_match(i, len, color);
            i += len;
            continue;
        }

        // Try named color (only at word boundary)
        let before = i.checked_sub(1).and_then(|p| chars.get(p).copied());
        let at_word_start = before.is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if at_word_start
            && ch.is_ascii_alphabetic()
            && let Some((color, len)) = try_named_color(chars.get(i..).unwrap_or(&[]))
        {
            push_match(i, len, color);
            i += len;
            continue;
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
    // Every function name starts with one of these; checking first spares the
    // copy of the window at every other position.
    if !matches!(chars.first(), Some('r' | 'R' | 'h' | 'H')) {
        return None;
    }
    let window = chars.get(..MAX_COLOR_FUNCTION_LEN).unwrap_or(chars);
    let name_len = window
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .count();
    let name: String = window
        .get(..name_len)?
        .iter()
        .map(char::to_ascii_lowercase)
        .collect();
    if window.get(name_len) != Some(&'(') {
        return None;
    }
    let is_rgb = match name.as_str() {
        "rgb" | "rgba" => true,
        "hsl" | "hsla" => false,
        _ => return None,
    };
    let has_alpha = name.ends_with('a');

    // Counted in characters, as the callers advance in characters.
    let close = name_len + 1 + window.get(name_len + 1..)?.iter().position(|&c| c == ')')?;
    let args: String = window.get(name_len + 1..close)?.iter().collect();

    let color = if is_rgb {
        parse_rgb_args(&args, has_alpha)?
    } else {
        parse_hsl_args(&args, has_alpha)?
    };

    Some((color, close + 1))
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
mod tests {
    use rstest::rstest;
    use tower_lsp::lsp_types::Position;

    use super::*;
    use crate::test_utils::parse_docs;

    fn colors_of(yaml: &str) -> Vec<ColorMatch> {
        find_colors(&parse_docs(yaml), yaml)
    }

    fn range(start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> Range {
        Range::new(
            Position::new(start_line, start_col),
            Position::new(end_line, end_col),
        )
    }

    /// The text a client would replace for a single-line `range`: columns are
    /// codepoints, lines end at `\n`, `\r\n`, or a lone `\r`.
    fn text_under(yaml: &str, range: Range) -> String {
        assert_eq!(range.start.line, range.end.line, "single-line range");
        yaml.replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .nth(range.start.line as usize)
            .expect("range line exists")
            .chars()
            .skip(range.start.character as usize)
            .take((range.end.character - range.start.character) as usize)
            .collect()
    }

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
    #[case::invalid_hex_gg("color: #gggggg\n")]
    #[case::unknown_named_color("color: notacolor\n")]
    // Comments produce no AST nodes, so no matches — deliberate behavior after AST retrofit.
    #[case::comment_line_skipped("# this is a comment with red\n")]
    #[case::block_literal_header_comment("a: | # red\n  x\n")]
    #[case::block_folded_header_comment("a: >- # red\n  x\n")]
    #[case::comment_after_double_quoted("a: \"x\" # red\n")]
    #[case::comment_after_single_quoted("a: 'x' # red\n")]
    #[case::escape_after_name("a: \"red\\x41\"\n")]
    #[case::escape_before_name("a: \"\\x41red\"\n")]
    #[case::unicode_escape_before_name("a: \"\\u0041red\"\n")]
    #[case::long_unicode_escape_after_name("a: \"red\\U00000041\"\n")]
    #[case::name_through_hex_escape("a: \"\\x72ed\"\n")]
    #[case::name_through_partial_escape("a: \"r\\x65d\"\n")]
    #[case::name_through_unicode_escapes("a: \"\\u0072\\u0065\\u0064\"\n")]
    #[case::hex_before_escape("a: \"#fff\\x41\"\n")]
    #[case::name_joined_by_escaped_break_before("a: \"a\\\n  red\"\n")]
    #[case::name_joined_by_escaped_break_after("a: \"red\\\n  s\"\n")]
    #[case::function_over_plain_line_break("a: rgb(1,\n  2, 3)\n")]
    #[case::function_over_single_quoted_line_break("a: 'rgb(1,\n  2, 3)'\n")]
    #[case::function_over_double_quoted_line_break("a: \"rgb(1,\n  2, 3)\"\n")]
    #[case::name_over_escaped_line_break("a: \"re\\\n  d\"\n")]
    #[case::hex_over_escaped_line_break("a: \"#ff\\\n  f\"\n")]
    #[case::function_over_escaped_line_break("a: \"rgb(1,\\\n  2, 3)\"\n")]
    #[case::function_over_folded_line_break("a: >\n  rgb(1,\n  2, 3)\n")]
    #[case::function_over_literal_line_break("a: |\n  rgb(1,\n  2, 3)\n")]
    #[case::name_over_plain_line_break("a: re\n  d\n")]
    #[case::empty_block_scalar("a: |\n")]
    #[case::empty_block_scalar_before_entry("a: |\nb: x\n")]
    fn find_colors_returns_empty(#[case] text: &str) {
        let colors = colors_of(text);
        assert!(colors.is_empty(), "expected no colors for: {text:?}");
    }

    // Each case lists `(line, start column, end column, source text)` of every
    // color in the order reported. The source text is what a client replaces,
    // so it is read back from the text under the range.
    #[rstest]
    // Single-line scalars
    #[case::plain("color: red\n", &[(0, 7, 10, "red")])]
    #[case::single_quoted("color: 'red'\n", &[(0, 8, 11, "red")])]
    #[case::double_quoted("color: \"red\"\n", &[(0, 8, 11, "red")])]
    #[case::single_quoted_hex("primary: '#ff0000'\n", &[(0, 10, 17, "#ff0000")])]
    #[case::double_quoted_hex("color: \"#ff0000\"\n", &[(0, 8, 15, "#ff0000")])]
    #[case::single_quoted_with_doubled_quote("c: 'it''s red'\n", &[(0, 10, 13, "red")])]
    #[case::two_colors_in_one_scalar("a: \"red red\"\n", &[(0, 4, 7, "red"), (0, 8, 11, "red")])]
    #[case::two_entries("a: red\nb: red\n", &[(0, 3, 6, "red"), (1, 3, 6, "red")])]
    #[case::quoted_key_is_no_value("'red': blue\n", &[(0, 7, 11, "blue")])]
    #[case::comment_after_plain("a: red # blue\n", &[(0, 3, 6, "red")])]
    #[case::comment_after_single_quoted("a: 'red' # blue\n", &[(0, 4, 7, "red")])]
    #[case::comment_after_double_quoted("a: \"red\" # blue\n", &[(0, 4, 7, "red")])]
    // Properties and aliases
    #[case::anchor_name_is_no_color("a: &red blue\n", &[(0, 8, 12, "blue")])]
    #[case::tag_before_value("a: !!str red\n", &[(0, 9, 12, "red")])]
    #[case::tag_name_is_no_color("a: !red blue\n", &[(0, 8, 12, "blue")])]
    #[case::alias_is_no_scalar("a: &x blue\nb: *x\n", &[(0, 6, 10, "blue")])]
    // Flow collections
    #[case::flow_sequence("[red, 'blue']\n", &[(0, 1, 4, "red"), (0, 7, 11, "blue")])]
    #[case::flow_mapping("{a: red, b: \"blue\"}\n", &[(0, 4, 7, "red"), (0, 13, 17, "blue")])]
    #[case::flow_sequence_over_lines("[ 'red',\n  \"blue\" ]\n", &[(0, 3, 6, "red"), (1, 3, 7, "blue")])]
    // Block scalars
    #[case::literal("a: |\n  red\n  blue\n", &[(1, 2, 5, "red"), (2, 2, 6, "blue")])]
    #[case::folded("a: >\n  red\n  blue\n", &[(1, 2, 5, "red"), (2, 2, 6, "blue")])]
    #[case::literal_with_indent_indicator("a: |2\n    red\n", &[(1, 4, 7, "red")])]
    #[case::hash_in_block_content("a: |\n  # red\n", &[(1, 4, 7, "red")])]
    #[case::block_with_blank_line("a: |\n  red\n\n  blue\n", &[(1, 2, 5, "red"), (3, 2, 6, "blue")])]
    #[case::nested_block("m:\n  a: |\n    red\n", &[(2, 4, 7, "red")])]
    #[case::block_in_sequence("- |\n  red\n", &[(1, 2, 5, "red")])]
    #[case::keep_chomping("a: |+\n  red\n\n\n", &[(1, 2, 5, "red")])]
    #[case::comment_after_block("a: |\n  red\n# blue\n", &[(1, 2, 5, "red")])]
    #[case::tab_in_block("a: |\n  \tred\n", &[(1, 3, 6, "red")])]
    #[case::block_with_tag("a: !!str |\n  red\n", &[(1, 2, 5, "red")])]
    #[case::block_with_anchor("a: &x |\n  red\n", &[(1, 2, 5, "red")])]
    #[case::multibyte_in_block("a: |\n  é red\n", &[(1, 4, 7, "red")])]
    // Multi-line plain and quoted scalars
    #[case::plain_over_lines("a: x\n  red\n  blue\n", &[(1, 2, 5, "red"), (2, 2, 6, "blue")])]
    #[case::plain_color_on_first_line("a: red\n  blue\n", &[(0, 3, 6, "red"), (1, 2, 6, "blue")])]
    #[case::double_quoted_over_lines("a: \"red\n  blue\"\n", &[(0, 4, 7, "red"), (1, 2, 6, "blue")])]
    #[case::double_quoted_blank_line("a: \"x\n\n  red\"\n", &[(2, 2, 5, "red")])]
    #[case::single_quoted_over_lines("a: 'x\n  red\n  blue'\n", &[(1, 2, 5, "red"), (2, 2, 6, "blue")])]
    // Multibyte characters count as one column each: the columns are codepoints,
    // the convention of every position of the server (`LineIndex`), not UTF-16
    // code units. A later move to UTF-16 changes the astral case on purpose.
    #[case::cyrillic_key("ключ: red\n", &[(0, 6, 9, "red")])]
    #[case::astral_character_before("a: \"😀 red\"\n", &[(0, 6, 9, "red")])]
    // Line breaks
    #[case::crlf_block("a: |\r\n  red\r\n  blue\r\n", &[(1, 2, 5, "red"), (2, 2, 6, "blue")])]
    #[case::crlf_plain("a: x\r\n  red\r\n  blue\r\n", &[(1, 2, 5, "red"), (2, 2, 6, "blue")])]
    #[case::crlf_quoted("a: \"x\r\n  red\"\r\n", &[(1, 2, 5, "red")])]
    #[case::lone_cr_block("a: |\r  red\r  blue\r", &[(1, 2, 5, "red"), (2, 2, 6, "blue")])]
    // Several documents
    #[case::second_document("a: red\n---\nb: |\n  blue\n", &[(0, 3, 6, "red"), (3, 2, 6, "blue")])]
    #[case::three_documents(
        "a: 'red'\n---\nb: \"blue\"\n---\nc: green\n",
        &[(0, 4, 7, "red"), (2, 4, 8, "blue"), (4, 3, 8, "green")]
    )]
    // A multibyte character inside a function counts as one column
    #[case::no_break_space_in_rgb(
        "a: rgb(1,\u{a0}2,3) x\n",
        &[(0, 3, 14, "rgb(1,\u{a0}2,3)")]
    )]
    #[case::color_after_function_with_no_break_space(
        "a: rgb(1,\u{a0}2,3) red\n",
        &[(0, 3, 14, "rgb(1,\u{a0}2,3)"), (0, 15, 18, "red")]
    )]
    #[case::ideographic_space_in_hsl(
        "a: hsl(0,\u{3000}100%, 50%)\n",
        &[(0, 3, 20, "hsl(0,\u{3000}100%, 50%)")]
    )]
    // Escapes of a double-quoted scalar that decode to no word character
    #[case::newline_escape_before("a: \"\\nred\"\n", &[(0, 6, 9, "red")])]
    #[case::newline_escape_after("a: \"red\\n\"\n", &[(0, 4, 7, "red")])]
    #[case::tab_escape_between("a: \"red\\tblue\"\n", &[(0, 4, 7, "red"), (0, 9, 13, "blue")])]
    #[case::escaped_quotes("a: \"\\\"red\\\"\"\n", &[(0, 6, 9, "red")])]
    #[case::escaped_backslash_after("a: \"red\\\\\"\n", &[(0, 4, 7, "red")])]
    #[case::space_escape_after("a: \"red\\x20\"\n", &[(0, 4, 7, "red")])]
    #[case::no_break_space_escape_before("a: \"\\u00a0red\"\n", &[(0, 10, 13, "red")])]
    #[case::escaped_break_after_space("a: \"x \\\n  red\"\n", &[(1, 2, 5, "red")])]
    #[case::escaped_break_before_comma(
        "a: \"red\\\n  , blue\"\n",
        &[(0, 4, 7, "red"), (1, 4, 8, "blue")]
    )]
    #[case::escaped_break_before_closing_quote("a: \"red\\\n  \"\n", &[(0, 4, 7, "red")])]
    #[case::hex_before_escaped_break_and_comma("a: \"#fff\\\n  , x\"\n", &[(0, 4, 8, "#fff")])]
    #[case::name_over_escaped_break_then_a_color_in_the_same_line(
        "a: \"r\\\n  ed blue\"\n",
        &[(1, 5, 9, "blue")]
    )]
    #[case::chain_of_escaped_breaks_before_comma(
        "a: \"red\\\n  \\\n  , x\"\n",
        &[(0, 4, 7, "red")]
    )]
    fn color_range_matches_source_position(
        #[case] text: &str,
        #[case] expected: &[(u32, u32, u32, &str)],
    ) {
        let colors = colors_of(text);
        let reported: Vec<(Range, String)> = colors
            .iter()
            .map(|c| (c.range, text_under(text, c.range)))
            .collect();
        let expected: Vec<(Range, String)> = expected
            .iter()
            .map(|&(line, from, to, token)| (range(line, from, line, to), token.to_owned()))
            .collect();
        assert_eq!(reported, expected, "colors of {text:?}");
    }

    #[test]
    fn color_function_longer_than_the_limit_is_not_reported() {
        let text = format!("a: rgb(1,{}2, 3)\n", " ".repeat(MAX_COLOR_FUNCTION_LEN));
        assert!(colors_of(&text).is_empty());
    }

    #[test]
    fn color_function_one_over_the_limit_is_not_reported() {
        // The function is "rgb(1,", the padding, and "2, 3)".
        let padding = MAX_COLOR_FUNCTION_LEN - "rgb(1,".len() - "2, 3)".len() + 1;
        let text = format!("a: rgb(1,{}2, 3)\n", " ".repeat(padding));
        assert!(colors_of(&text).is_empty());
    }

    #[test]
    fn color_function_up_to_the_limit_is_reported() {
        let padding = MAX_COLOR_FUNCTION_LEN - "rgb(1,".len() - "2, 3)".len();
        let text = format!("a: rgb(1,{}2, 3)\n", " ".repeat(padding));
        let colors = colors_of(&text);
        assert_eq!(colors.len(), 1);
        assert_eq!(text_under(&text, colors[0].range).chars().count(), 128);
    }

    // The scans of large scalars finish: no quadratic work in the size of a
    // line or a scalar. Nothing here asserts a time; a quadratic scan does not
    // end in any useful time at these sizes.
    #[rstest]
    #[case::one_long_plain_line(format!("a: {}\n", "rgb(".repeat(100_000)))]
    #[case::folded_scalar_of_many_lines(format!("a: >\n{}", "  rgb(rgb(rgb(\n".repeat(50_000)))]
    #[case::plain_scalar_of_many_short_lines(format!("a: x\n{}", "  r\n".repeat(100_000)))]
    #[case::literal_scalar_of_many_lines(format!("a: |\n{}", "  rgb(rgb(\n".repeat(50_000)))]
    #[case::quoted_scalar_of_many_escaped_breaks(format!("a: \"x\\\n{}  \"\n", "  \\\n".repeat(100_000)))]
    #[case::quoted_scalar_of_many_lines(format!("a: \"x\n{}\"\n", "  hsl(hsl(\n".repeat(50_000)))]
    fn large_scalars_without_colors_are_scanned_in_linear_time(#[case] text: String) {
        assert!(colors_of(&text).is_empty());
    }

    #[test]
    fn large_folded_scalar_reports_every_color() {
        let text = format!("a: >\n{}", "  red rgb(1, 2, 3)\n".repeat(20_000));
        assert_eq!(colors_of(&text).len(), 40_000);
    }

    #[test]
    fn colors_in_block_scalar_keep_their_order_and_values() {
        let colors = colors_of("a: |\n  red\n  blue\n");
        assert_eq!(colors.len(), 2);
        assert!(color_eq(
            &colors[0].color,
            &Color {
                red: 1.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0
            }
        ));
        assert!(color_eq(
            &colors[1].color,
            &Color {
                red: 0.0,
                green: 0.0,
                blue: 1.0,
                alpha: 1.0
            }
        ));
    }

    #[test]
    fn text_shorter_than_the_spans_reports_nothing() {
        let docs = parse_docs("a: red\n");
        assert!(find_colors(&docs, "a").is_empty());
    }

    #[test]
    fn span_start_inside_a_multibyte_character_reports_nothing() {
        let docs = parse_docs("a: red\n");
        // The scalar starts at byte 3, in the middle of the second "é".
        assert!(find_colors(&docs, "ééé").is_empty());
    }

    #[test]
    fn no_documents_report_nothing() {
        assert!(find_colors(&[], "").is_empty());
    }

    #[test]
    fn byte_order_mark_does_not_shift_the_range_from_the_scalars_position() {
        // The offsets of the spans and of the LineIndex both count the BOM,
        // so the color sits where `span_to_lsp` puts its scalar.
        let text = "\u{feff}color: red\n";
        let colors = colors_of(text);
        assert_eq!(colors.len(), 1);
        let docs = parse_docs(text);
        let Node::Mapping { entries, .. } = &docs[0].root else {
            panic!("a mapping");
        };
        let Node::Scalar { loc, .. } = &entries[0].1 else {
            panic!("a scalar");
        };
        assert_eq!(
            colors[0].range,
            span_to_lsp(*loc, docs[0].line_index()),
            "the color is the whole scalar"
        );
    }

    // ── Hex colors ──────────────────────────────────────────────────────────

    #[test]
    fn hex_3_digit_expands_correctly() {
        // Hex colors must be quoted in YAML — unquoted # is a comment marker.
        let colors = colors_of("color: '#fff'\n");
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
        // Hex colors must be quoted in YAML — unquoted # is a comment marker.
        let colors = colors_of("color: '#FF0000'\n");
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
        // Hex colors must be quoted in YAML — unquoted # is a comment marker.
        let colors = colors_of("color: '#00ff00ff'\n");
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
        let colors = colors_of("color: red\n");
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
        let colors = colors_of("bg: aliceblue\n");
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
        let colors = colors_of("color: RED\n");
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
        let colors = colors_of("color: rgb(255, 0, 0)\n");
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
        let colors = colors_of("color: rgb(100%, 0%, 0%)\n");
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
        let colors = colors_of("color: rgba(0, 0, 0, 0.5)\n");
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
        let colors = colors_of("color: hsl(0, 100%, 50%)\n");
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
        let colors = colors_of("color: hsla(120, 100%, 50%, 0.5)\n");
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
        let colors = colors_of("red: blue\n");
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
        // Hex colors must be quoted in YAML — unquoted # is a comment marker.
        let colors = colors_of("color: '#ff0000'\n");
        assert_eq!(colors.len(), 1);
    }

    #[test]
    fn range_positions_correct() {
        let colors = colors_of("color: red\n");
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].range.start.line, 0);
        // "color: " is 7 chars, so "red" starts at column 7
        assert_eq!(colors[0].range.start.character, 7);
        assert_eq!(colors[0].range.end.character, 10);
    }

    // ── Required regression cases ────────────────────────────────────────────

    #[test]
    fn hex_in_value_position_produces_correct_range() {
        // Hex must be quoted to avoid YAML treating # as comment.
        // "color: '#ff0000'" — the opening quote is at column 7, so the
        // 7-character color starts at column 8 and ends at column 15.
        let colors = colors_of("color: '#ff0000'\n");
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].range.start.line, 0);
        assert_eq!(colors[0].range.start.character, 8);
        assert_eq!(colors[0].range.end.character, 15);
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
    fn css_named_color_in_value_position_is_detected() {
        // "color: blue" — value scalar "blue" at column 7
        let colors = colors_of("color: blue\n");
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].range.start.line, 0);
        assert_eq!(colors[0].range.start.character, 7);
        assert_eq!(colors[0].range.end.character, 11);
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
    fn quoted_string_color_value_produces_correct_range() {
        // "color: 'red'" — the opening quote is at column 7; the color itself
        // (without the quotes) spans columns 8 to 11.
        let colors = colors_of("color: 'red'\n");
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].range.start.line, 0);
        assert_eq!(colors[0].range.start.character, 8);
        assert_eq!(colors[0].range.end.character, 11);
    }

    #[test]
    fn hex_color_in_comment_produces_no_match() {
        // A comment-only document produces zero AST nodes; no colors can match.
        let colors = colors_of("# #ff0000\n");
        assert!(colors.is_empty(), "comment colors must not be detected");
    }

    #[test]
    fn color_pattern_as_mapping_key_is_not_matched() {
        // "red: true" — key "red" must NOT produce a match; value "true" is not a color.
        let colors = colors_of("red: true\n");
        assert!(
            colors.is_empty(),
            "mapping key color pattern must not be matched; got {colors:?} matches",
            colors = colors.len()
        );
    }

    // ── Additional regression cases ──────────────────────────────────────────

    #[test]
    fn color_in_sequence_item_is_detected() {
        let colors = colors_of("colors:\n  - red\n  - blue\n");
        assert_eq!(
            colors.len(),
            2,
            "both sequence item colors must be detected"
        );
    }

    #[test]
    fn freestanding_top_level_scalar_is_scanned() {
        // A bare document scalar is a value and must be scanned.
        let colors = colors_of("red\n");
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
    fn color_in_nested_mapping_value_is_detected() {
        let colors = colors_of("theme:\n  primary: blue\n");
        assert_eq!(colors.len(), 1);
        assert_eq!(colors[0].range.start.line, 1);
        assert_eq!(colors[0].range.start.character, 11);
        assert_eq!(colors[0].range.end.character, 15);
    }

    #[test]
    fn multi_line_scalar_with_tokens_on_each_line() {
        // Block literal scalar with multiple color names on separate lines.
        // Tokens on single lines are found; none straddle a '\n' boundary.
        let colors = colors_of("color: |\n  red\n  blue\n");
        assert_eq!(
            colors.len(),
            2,
            "both colors on separate lines must be detected"
        );
        assert_eq!(
            colors.iter().map(|c| c.range).collect::<Vec<_>>(),
            vec![range(1, 2, 1, 5), range(2, 2, 2, 6)]
        );
    }
}
