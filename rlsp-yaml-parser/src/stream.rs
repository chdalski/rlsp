// SPDX-License-Identifier: MIT

//! YAML 1.2 §9 document stream productions [202]–[211].
//!
//! Covers document markers, forbidden positions, document prefixes, bare
//! documents, explicit documents, directive documents, and the full YAML
//! stream.  Each function is named after the spec production and
//! cross-referenced by its production number in a `// [N]` comment.
//!
//! The public entry point is [`tokenize`], which takes a YAML string slice
//! and returns all tokens produced by parsing the entire stream.

use crate::block::s_l_block_node;
use crate::chars::b_break;
use crate::combinator::{
    Context, Parser, Reply, State, alt, char_parser, many0, many1, neg_lookahead, opt, seq,
    wrap_tokens,
};
use crate::structure::{c_forbidden, l_directive, s_l_comments};
use crate::token::{Code, Token};

// ---------------------------------------------------------------------------
// §9.1 – Document boundary markers [202]–[203]
// ---------------------------------------------------------------------------

/// [202] c-directives-end — `---` at the start of a line.
///
/// Emits a single `DirectivesEnd` token. The `---` must be followed by
/// whitespace, a line break, or EOF to be a valid document marker (otherwise
/// it could be content like `---word`).
#[must_use]
pub fn c_directives_end() -> Parser<'static> {
    Box::new(|state| {
        use crate::combinator::Reply;
        let dashes = seq(char_parser('-'), seq(char_parser('-'), char_parser('-')));
        match dashes(state) {
            Reply::Success { state: after, .. } => {
                // Must be followed by whitespace, break, or EOF.
                match after.peek() {
                    None | Some(' ' | '\t' | '\n' | '\r') => Reply::Success {
                        tokens: vec![crate::token::Token {
                            code: Code::DirectivesEnd,
                            pos: after.pos,
                            text: "",
                        }],
                        state: after,
                    },
                    Some(_) => Reply::Failure,
                }
            }
            other @ (Reply::Failure | Reply::Error(_)) => other,
        }
    })
}

/// [203] c-document-end — `...` at the start of a line.
///
/// Emits a single `DocumentEnd` token. Like `c-directives-end`, requires
/// whitespace, break, or EOF after the marker.
#[must_use]
pub fn c_document_end() -> Parser<'static> {
    Box::new(|state| {
        use crate::combinator::Reply;
        let dots = seq(char_parser('.'), seq(char_parser('.'), char_parser('.')));
        match dots(state) {
            Reply::Success { state: after, .. } => match after.peek() {
                None | Some(' ' | '\t' | '\n' | '\r') => Reply::Success {
                    tokens: vec![crate::token::Token {
                        code: Code::DocumentEnd,
                        pos: after.pos,
                        text: "",
                    }],
                    state: after,
                },
                Some(_) => Reply::Failure,
            },
            other @ (Reply::Failure | Reply::Error(_)) => other,
        }
    })
}

// ---------------------------------------------------------------------------
// §9.2 – Document prefix [204]
// ---------------------------------------------------------------------------

/// [204] l-document-prefix — optional BOM then zero or more comment lines.
///
/// The BOM (`\u{FEFF}`) may appear at the start of the stream to indicate
/// encoding.  After the optional BOM, any number of comment-or-blank lines
/// may precede the first document.
#[must_use]
pub fn l_document_prefix() -> Parser<'static> {
    seq(opt(char_parser('\u{FEFF}')), many0(l_comment_line()))
}

/// A comment, blank, or whitespace-only line in a document prefix.
/// Whitespace before `#` is consumed. Whitespace-only lines (spaces/tabs) are
/// also consumed here since they cannot be block scalar content before `---`.
fn l_comment_line() -> Parser<'static> {
    use crate::chars::s_white;
    alt(
        seq(many0(s_white()), seq(c_nb_comment_text(), b_break())),
        alt(seq(many1(s_white()), b_break()), b_break()),
    )
}

/// `# <non-break-chars>*` — comment text without the leading whitespace.
fn c_nb_comment_text() -> Parser<'static> {
    seq(char_parser('#'), many0(crate::chars::nb_char()))
}

// ---------------------------------------------------------------------------
// §9.2 – Documents [206]–[208]
// ---------------------------------------------------------------------------

/// [206] l-bare-document — a document with no `---` prefix.
///
/// The document content is a block node at indentation −1 (top level).
/// The parser stops before any `c-forbidden` position so that document
/// boundaries inside a stream are respected.
///
/// Emits `BeginDocument` / content tokens / `EndDocument`.
#[must_use]
pub fn l_bare_document() -> Parser<'static> {
    wrap_tokens(
        Code::BeginDocument,
        Code::EndDocument,
        seq(
            neg_lookahead(c_forbidden()),
            s_l_block_node(-1, Context::BlockIn),
        ),
    )
}

/// [207] l-explicit-document — a document introduced by `---`.
///
/// The `---` marker is followed by optional block content.
///
/// Emits `BeginDocument` / `DirectivesEnd` token / optional content /
/// `EndDocument`.
#[must_use]
pub fn l_explicit_document() -> Parser<'static> {
    wrap_tokens(
        Code::BeginDocument,
        Code::EndDocument,
        seq(c_directives_end(), opt(l_document_content())),
    )
}

/// Content of an explicit document: block node or empty node with comments.
fn l_document_content() -> Parser<'static> {
    alt(s_l_block_node(-1, Context::BlockIn), s_l_comments())
}

/// [208] l-directive-document — one or more directives followed by `---`.
///
/// Emits `BeginDocument` / directive tokens / `DirectivesEnd` token /
/// optional content / optional `DocumentEnd` token / `EndDocument`.
///
/// Rejects duplicate `%YAML` directives per spec §6.8.1: "at most one YAML
/// directive" per document.
#[must_use]
pub fn l_directive_document() -> Parser<'static> {
    wrap_tokens(
        Code::BeginDocument,
        Code::EndDocument,
        seq(
            directives_no_dup_yaml(),
            seq(
                many0_progressing(l_document_prefix()),
                l_explicit_document_body(),
            ),
        ),
    )
}

/// Parse one or more directives, rejecting duplicate `%YAML` directives.
fn directives_no_dup_yaml() -> Parser<'static> {
    Box::new(|state| {
        let mut all_tokens = Vec::new();
        let mut current = state;
        let mut yaml_seen = false;
        let mut count = 0;

        loop {
            // Check if this is a %YAML directive before parsing.
            let is_yaml = current.input.starts_with("%YAML")
                && current
                    .input
                    .as_bytes()
                    .get(5)
                    .is_some_and(|&b| b == b' ' || b == b'\t');

            match l_directive()(current.clone()) {
                Reply::Success { tokens, state } => {
                    if is_yaml {
                        if yaml_seen {
                            // Duplicate %YAML — reject the entire directive block.
                            return Reply::Failure;
                        }
                        yaml_seen = true;
                    }
                    all_tokens.extend(tokens);
                    current = state;
                    count += 1;
                }
                Reply::Failure | Reply::Error(_) => break,
            }
        }

        if count == 0 {
            return Reply::Failure;
        }
        Reply::Success {
            tokens: all_tokens,
            state: current,
        }
    })
}

/// The body of an explicit document (everything after the directives).
///
/// This mirrors `l-explicit-document` but without the outer `BeginDocument`/
/// `EndDocument` wrap, since `l-directive-document` owns that wrapper.
fn l_explicit_document_body() -> Parser<'static> {
    seq(c_directives_end(), opt(l_document_content()))
}

// ---------------------------------------------------------------------------
// §9.2 – Document stream [209]–[211]
// ---------------------------------------------------------------------------

/// [209] l-any-document — directive document, explicit document, or bare document.
#[must_use]
pub fn l_any_document() -> Parser<'static> {
    alt(
        l_directive_document(),
        alt(l_explicit_document(), l_bare_document()),
    )
}

/// [210] l-document-suffix — `...` document-end marker with trailing comments.
#[must_use]
pub fn l_document_suffix() -> Parser<'static> {
    seq(c_document_end(), s_l_comments())
}

/// [211] l-yaml-stream — a YAML byte stream: zero or more documents.
///
/// Per spec: `l-document-prefix* l-any-document?
///            ( l-document-suffix+ l-document-prefix* l-any-document?
///            | l-document-prefix* l-explicit-document? )*`
///
/// After each document, either document-suffix(es) `...` or more document
/// prefixes may appear. `l-document-prefix` can succeed consuming zero bytes,
/// so we use progress-guarded repetition to prevent infinite loops.
#[must_use]
pub fn l_yaml_stream() -> Parser<'static> {
    Box::new(|state| {
        let mut all_tokens: Vec<crate::token::Token<'static>> = Vec::new();
        let mut current = state;

        // l-document-prefix*
        if let Reply::Success { tokens, state } =
            many0_progressing(l_document_prefix())(current.clone())
        {
            all_tokens.extend(tokens);
            current = state;
        }

        // First position: eof | c-document-end | l-any-document
        if current.input.is_empty() {
            // EOF
        } else if let Reply::Success { tokens, state } = l_document_suffix()(current.clone()) {
            all_tokens.extend(tokens);
            current = state;
        } else if let Reply::Success { tokens, state } = l_any_document()(current.clone()) {
            all_tokens.extend(tokens);
            current = state;
        }

        // Continuation per spec [211]:
        //   ( l-document-suffix+ l-document-prefix* l-any-document?
        //   | l-document-prefix* l-explicit-document? )*
        loop {
            let before = current.pos.byte_offset;

            // Branch 1: suffix(es) then prefix(es) then any document
            if let Reply::Success { tokens, state } = many1(l_document_suffix())(current.clone()) {
                let mut bt = tokens;
                let mut bs = state;
                if let Reply::Success { tokens, state } =
                    many0_progressing(l_document_prefix())(bs.clone())
                {
                    bt.extend(tokens);
                    bs = state;
                }
                if let Reply::Success { tokens, state } = l_any_document()(bs.clone()) {
                    bt.extend(tokens);
                    bs = state;
                }
                if bs.pos.byte_offset > before {
                    all_tokens.extend(bt);
                    current = bs;
                    continue;
                }
            }

            // Branch 2: prefix(es) then optional explicit document
            {
                let mut bt: Vec<crate::token::Token<'static>> = Vec::new();
                let mut bs = current.clone();
                if let Reply::Success { tokens, state } =
                    many0_progressing(l_document_prefix())(bs.clone())
                {
                    bt.extend(tokens);
                    bs = state;
                }
                if let Reply::Success { tokens, state } = l_explicit_document()(bs.clone()) {
                    bt.extend(tokens);
                    bs = state;
                }
                if bs.pos.byte_offset > before {
                    all_tokens.extend(bt);
                    current = bs;
                    continue;
                }
            }

            break;
        }

        Reply::Success {
            tokens: all_tokens,
            state: current,
        }
    })
}

/// Like `many0` but only repeats when the parser consumed at least one byte.
///
/// Prevents infinite loops when the inner parser succeeds without consuming
/// any input (e.g. `l-document-prefix` on empty input).
fn many0_progressing(p: Parser<'static>) -> Parser<'static> {
    Box::new(move |mut state: State<'static>| {
        let mut all_tokens: Vec<crate::token::Token<'static>> = Vec::new();
        loop {
            let before = state.pos.byte_offset;
            match p(state.clone()) {
                Reply::Failure | Reply::Error(_) => break,
                Reply::Success {
                    tokens,
                    state: next,
                } => {
                    if next.pos.byte_offset == before {
                        // No progress — stop to avoid infinite loop.
                        break;
                    }
                    all_tokens.extend(tokens);
                    state = next;
                }
            }
        }
        Reply::Success {
            tokens: all_tokens,
            state,
        }
    })
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Tokenize a YAML string, returning all tokens produced by parsing the
/// complete document stream.
///
/// Returns an empty `Vec` when the input is empty.  Tokens include all
/// structural markers (`BeginDocument`, `EndDocument`, `DirectivesEnd`, etc.)
/// as well as content tokens (`Text`, `Indicator`, etc.).
///
/// When the parser fails or does not consume all input, a final
/// `Code::Error` token is appended at the error/remainder position.
/// Callers (notably `parse_events`) treat this token as a signal to emit
/// a parse error.
#[must_use]
pub fn tokenize(input: &str) -> Vec<Token<'_>> {
    // `Parser<'static>` (the type returned by combinators in this module)
    // only accepts `State<'static>` at call sites, because `'static` appears
    // in both the closure's capture set and its argument type and Rust cannot
    // prove the argument `'i` outlives `'static` for an arbitrary `&'i str`.
    //
    // This transmute is safe because:
    //   (a) The extended `'static` reference is not stored anywhere — it is
    //       consumed within the `l_yaml_stream()` call and never escapes.
    //   (b) All token `text` fields point into `input`; transmuting
    //       `Vec<Token<'static>>` back to `Vec<Token<'i>>` is sound because
    //       the tokens' provenance is exactly `input`.
    //   (c) The original `input` remains live for the duration of this call,
    //       so the extended reference is never dangling.
    let extended: &'static str = unsafe { &*std::ptr::from_ref::<str>(input) };
    let state: State<'static> = State::new(extended);
    match l_yaml_stream()(state) {
        Reply::Success { mut tokens, state } => {
            if !state.input.is_empty()
                && !state
                    .input
                    .chars()
                    .all(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r'))
            {
                // Parser consumed some input then stopped — remaining content is
                // invalid YAML. Trailing whitespace at EOF is tolerated.
                tokens.push(Token {
                    code: Code::Error,
                    pos: state.pos,
                    text: state.input,
                });
            }
            validate_tokens(extended, &mut tokens);
            unsafe {
                // Shrink token lifetime back to `'i` (= lifetime of `input`).
                core::mem::transmute::<Vec<Token<'static>>, Vec<Token<'_>>>(tokens)
            }
        }
        Reply::Failure | Reply::Error(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Post-parse validation
// ---------------------------------------------------------------------------

/// Reject patterns that the PEG parser accepts but the YAML spec forbids.
#[allow(
    clippy::too_many_lines,
    clippy::indexing_slicing,
    clippy::manual_strip,
    clippy::char_lit_as_u8,
    clippy::wildcard_enum_match_arm
)]
fn validate_tokens<'a>(input: &'a str, tokens: &mut Vec<Token<'a>>) {
    if tokens.iter().any(|t| t.code == Code::Error) {
        return;
    }
    let err = |byte: usize| Token {
        code: Code::Error,
        pos: crate::pos::Pos {
            byte_offset: byte,
            char_offset: byte,
            line: 0,
            column: 0,
        },
        text: "",
    };

    // Collect byte ranges of quoted scalars, flow collections, block scalars.
    let mut quoted: Vec<(usize, usize)> = Vec::new();
    let mut flow_stack: Vec<usize> = Vec::new();
    let mut flow: Vec<(usize, usize)> = Vec::new();
    let mut block_scalars: Vec<(usize, usize)> = Vec::new();
    let mut scalar_open: Option<usize> = None;
    let mut is_block = false;

    for t in tokens.iter() {
        match t.code {
            Code::BeginScalar => {
                scalar_open = Some(t.pos.byte_offset);
                is_block = false;
            }
            Code::Indicator if matches!(t.text, "|" | ">") && scalar_open.is_some() => {
                is_block = true;
            }
            Code::EndScalar => {
                if let Some(start) = scalar_open.take() {
                    let end = t.pos.byte_offset;
                    if is_block {
                        block_scalars.push((start, end));
                    } else if start < end
                        && input
                            .as_bytes()
                            .get(start)
                            .is_some_and(|&b| b == b'\'' || b == b'"')
                    {
                        quoted.push((start, end));
                    }
                }
                is_block = false;
            }
            Code::Indicator if matches!(t.text, "[" | "{") => {
                flow_stack.push(t.pos.byte_offset);
            }
            Code::Indicator if matches!(t.text, "]" | "}") => {
                if let Some(s) = flow_stack.pop() {
                    flow.push((s, t.pos.byte_offset + 1));
                }
            }
            _ => {}
        }
    }

    let in_any =
        |byte: usize, ranges: &[(usize, usize)]| ranges.iter().any(|&(s, e)| byte > s && byte < e);
    let lines: Vec<&str> = input.lines().collect();

    // Check 1: doc markers at column 0 inside quoted scalars (5TRB, RXY3, 9MQT).
    let mut offset = 0;
    for (i, line) in lines.iter().enumerate() {
        if i > 0
            && (line.starts_with("---") || line.starts_with("..."))
            && line
                .as_bytes()
                .get(3)
                .is_none_or(|&b| matches!(b, b' ' | b'\t'))
            && in_any(offset, &quoted)
        {
            tokens.push(err(offset));
            return;
        }
        offset += line.len() + 1;
    }

    // Check 2: doc markers at column 0 inside flow collections (N782).
    offset = 0;
    for (i, line) in lines.iter().enumerate() {
        if i > 0
            && (line.starts_with("---") || line.starts_with("..."))
            && line
                .as_bytes()
                .get(3)
                .is_none_or(|&b| matches!(b, b' ' | b'\t'))
            && in_any(offset, &flow)
        {
            tokens.push(err(offset));
            return;
        }
        offset += line.len() + 1;
    }

    // Check 3: tabs as indentation in block scalars (Y79Y).
    for &(start, end) in &block_scalars {
        let slice = &input[start..end.min(input.len())];
        let mut inner_off = 0;
        for (i, line) in slice.split('\n').enumerate() {
            if i > 0 && line.starts_with('\t') {
                tokens.push(err(start + inner_off));
                return;
            }
            inner_off += line.len() + 1;
        }
    }

    // Check 4: nested implicit mappings on same line (ZCZ6, ZL4Z, 5U3A).
    offset = 0;
    for line in &lines {
        let trimmed = line.trim_start();
        let in_bs = block_scalars.iter().any(|&(s, e)| offset > s && offset < e);
        if !trimmed.starts_with('?')
            && !trimmed.starts_with(':')
            && !trimmed.starts_with('|')
            && !trimmed.starts_with('>')
            && !trimmed.starts_with('#')
            && !trimmed.starts_with('-')
            && !trimmed.starts_with('&')
            && !in_bs
        {
            let bytes = line.as_bytes();
            let mut colons = 0u32;
            let mut j = 0;
            while j < bytes.len() {
                let abs = offset + j;
                if in_any(abs, &quoted) || in_any(abs, &flow) {
                    j += 1;
                    continue;
                }
                if matches!(bytes[j], b'\'' | b'"') {
                    let q = bytes[j];
                    j += 1;
                    while j < bytes.len() {
                        if bytes[j] == q {
                            if q == b'\'' && j + 1 < bytes.len() && bytes[j + 1] == b'\'' {
                                j += 2;
                            } else {
                                j += 1;
                                break;
                            }
                        } else {
                            if bytes[j] == b'\\' && q == b'"' {
                                j += 1;
                            }
                            j += 1;
                        }
                    }
                    continue;
                }
                if bytes[j] == b'&' {
                    j += 1;
                    while j < bytes.len() && !matches!(bytes[j], b' ' | b'\t') {
                        j += 1;
                    }
                    continue;
                }
                if bytes[j] == b':' && j + 1 < bytes.len() && bytes[j + 1] == b' ' {
                    colons += 1;
                    if colons > 1 {
                        tokens.push(err(abs));
                        return;
                    }
                }
                if colons == 1
                    && bytes[j] == b'-'
                    && j + 1 < bytes.len()
                    && bytes[j + 1] == b' '
                    && j > 0
                    && bytes[j - 1] == b' '
                {
                    tokens.push(err(abs));
                    return;
                }
                j += 1;
            }
        }
        offset += line.len() + 1;
    }

    // Check 5: anchor before "- " on same line (SY6V).
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with('&') {
            let end = trimmed[1..]
                .find([' ', '\t'])
                .map_or(trimmed.len(), |p| p + 1);
            let after = trimmed[end..].trim_start();
            if after.starts_with("- ") || after == "-" {
                let off = line.as_ptr() as usize - input.as_ptr() as usize;
                tokens.push(err(off));
                return;
            }
        }
    }

    // Check 6: block scalar indent — all-spaces line before lower-indent content (W9L4, S98Z).
    // Find block scalar indicators (| or >) which may appear after a key.
    for (i, line) in lines.iter().enumerate() {
        // Find | or > that starts a block scalar (after `: ` or at start).
        let indicator_pos = line.rfind(['|', '>']);
        let Some(ip) = indicator_pos else {
            continue;
        };
        // Verify it looks like a block scalar indicator (preceded by space/colon or at start).
        if ip > 0 && !matches!(line.as_bytes()[ip - 1], b' ' | b'\t') {
            continue;
        }
        // After the indicator, only chomping/indent chars and comment allowed.
        let after_ind = &line[ip + 1..];
        let after_trimmed =
            after_ind.trim_start_matches(|ch: char| matches!(ch, '+' | '-' | '0'..='9'));
        if !after_trimmed.is_empty()
            && !after_trimmed.starts_with(' ')
            && !after_trimmed.starts_with('\t')
            && !after_trimmed.starts_with('#')
        {
            continue;
        }
        let base = line.len() - line.trim_start().len();
        let mut first_blank_sp: Option<usize> = None;
        for j in (i + 1)..lines.len() {
            let cl = lines[j];
            if cl.is_empty() {
                continue;
            }
            let sp = cl.chars().take_while(|&ch| ch == ' ').count();
            let rest = &cl[sp..];
            if rest.is_empty() || rest == "\r" {
                if first_blank_sp.is_none() && sp > base {
                    first_blank_sp = Some(sp);
                }
                continue;
            }
            if sp <= base {
                break;
            }
            if let Some(fbs) = first_blank_sp {
                if fbs > sp {
                    let off: usize = lines[..j].iter().map(|l| l.len() + 1).sum();
                    tokens.push(err(off));
                    return;
                }
            }
            break;
        }
    }

    // Check 7: tag handle scope per document (QLJ7).
    {
        let mut handles: Vec<String> = vec!["!".into(), "!!".into()];
        let mut past_dir = false;
        offset = 0;
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.starts_with("%TAG ") {
                let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
                if parts.len() >= 2 {
                    handles.push(parts[1].to_string());
                }
            } else if trimmed == "---" || trimmed.starts_with("--- ") {
                if past_dir {
                    handles = vec!["!".into(), "!!".into()];
                }
                past_dir = true;
                // Check tag usage on the --- line itself (e.g., "--- !prefix!A").
                if let Some(rest) = trimmed.strip_prefix("--- ") {
                    let rb = rest.as_bytes();
                    let mut rj = 0;
                    while rj < rb.len() {
                        if rb[rj] == b'!'
                            && rj + 1 < rb.len()
                            && rb[rj + 1] != b' '
                            && rb[rj + 1] != b'<'
                        {
                            let hs = rj;
                            rj += 1;
                            while rj < rb.len()
                                && rb[rj] != b'!'
                                && rb[rj] != b' '
                                && rb[rj] != b'\t'
                            {
                                rj += 1;
                            }
                            if rj < rb.len() && rb[rj] == b'!' {
                                rj += 1;
                                let handle = &rest[hs..rj];
                                if handle != "!"
                                    && handle != "!!"
                                    && !handles.contains(&handle.to_string())
                                {
                                    let line_off = line.as_ptr() as usize - input.as_ptr() as usize;
                                    tokens.push(err(line_off + 4 + hs));
                                    return;
                                }
                            }
                            continue;
                        }
                        rj += 1;
                    }
                }
            } else if trimmed == "..." || trimmed.starts_with("... ") {
                handles = vec!["!".into(), "!!".into()];
                past_dir = false;
            } else if past_dir {
                let bytes = line.as_bytes();
                let mut j = 0;
                while j < bytes.len() {
                    if bytes[j] == b'!'
                        && j + 1 < bytes.len()
                        && bytes[j + 1] != b' '
                        && bytes[j + 1] != b'<'
                    {
                        let hs = j;
                        j += 1;
                        while j < bytes.len()
                            && bytes[j] != b'!'
                            && bytes[j] != b' '
                            && bytes[j] != b'\t'
                        {
                            j += 1;
                        }
                        if j < bytes.len() && bytes[j] == b'!' {
                            j += 1;
                            let handle = &line[hs..j];
                            if handle != "!"
                                && handle != "!!"
                                && !handles.contains(&handle.to_string())
                            {
                                tokens.push(err(offset + hs));
                                return;
                            }
                        }
                        continue;
                    }
                    j += 1;
                }
            }
            offset += line.len() + 1;
        }
    }

    // Check 9: mapping on doc-start line with anchor (CXX2).
    for line in &lines {
        if line.starts_with("--- ") {
            let after = line[4..].trim_start();
            if after.starts_with('&') {
                if let Some(space) = after[1..].find(' ') {
                    let rest = after[space + 2..].trim_start();
                    if rest.contains(": ") || rest.ends_with(':') {
                        let off = line.as_ptr() as usize - input.as_ptr() as usize;
                        tokens.push(err(off));
                        return;
                    }
                }
            }
        }
    }

    // Check 10: anchor at col 0 before seq entry in mapping context (G9HC).
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with('&') && !line.contains(' ') && !line.contains('\t') {
            if let Some(next) = lines.get(i + 1) {
                if next.starts_with("- ") || *next == "-" {
                    let has_mapping = lines[..i].iter().any(|p| {
                        let t = p.trim_start();
                        t.contains(": ") || t.ends_with(':')
                    });
                    if has_mapping {
                        let off: usize = lines[..i].iter().map(|l| l.len() + 1).sum();
                        tokens.push(err(off));
                        return;
                    }
                }
            }
        }
    }

    // Checks for ZXT5 and S98Z are in event.rs::validate_input, which runs
    // as a pre-event validation pass. Token injection here does not propagate
    // through the event layer.
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used,
    unused_imports
)]
mod tests {
    use super::*;
    use crate::combinator::{Context, Reply, State};
    use crate::token::Code;

    fn state(input: &str) -> State<'_> {
        State::new(input)
    }

    fn is_success(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Success { .. })
    }

    fn is_failure(reply: &Reply<'_>) -> bool {
        matches!(reply, Reply::Failure)
    }

    fn remaining<'a>(reply: &'a Reply<'a>) -> &'a str {
        match reply {
            Reply::Success { state, .. } => state.input,
            Reply::Failure | Reply::Error(_) => panic!("expected success, got failure/error"),
        }
    }

    fn codes(reply: Reply<'_>) -> Vec<Code> {
        match reply {
            Reply::Success { tokens, .. } => tokens.into_iter().map(|t| t.code).collect(),
            Reply::Failure | Reply::Error(_) => panic!("expected success"),
        }
    }

    fn has_code(reply: &Reply<'_>, code: Code) -> bool {
        match reply {
            Reply::Success { tokens, .. } => tokens.iter().any(|t| t.code == code),
            Reply::Failure | Reply::Error(_) => false,
        }
    }

    // -----------------------------------------------------------------------
    // Group 1: c_directives_end [202] and c_document_end [203]
    // -----------------------------------------------------------------------

    #[test]
    fn c_directives_end_matches_triple_dash() {
        let reply = c_directives_end()(state("---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "\n");
    }

    #[test]
    fn c_directives_end_emits_directives_end_token() {
        let reply = c_directives_end()(state("---\n"));
        assert!(has_code(&reply, Code::DirectivesEnd));
    }

    #[test]
    fn c_directives_end_fails_on_non_dash() {
        let reply = c_directives_end()(state("abc"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_directives_end_fails_on_partial_dash() {
        let reply = c_directives_end()(state("--a"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_document_end_matches_triple_dot() {
        let reply = c_document_end()(state("...\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "\n");
    }

    #[test]
    fn c_document_end_emits_document_end_token() {
        let reply = c_document_end()(state("...\n"));
        assert!(has_code(&reply, Code::DocumentEnd));
    }

    // -----------------------------------------------------------------------
    // Group 2: c_forbidden [205]
    // -----------------------------------------------------------------------

    #[test]
    fn c_forbidden_triggers_on_triple_dash_followed_by_space() {
        let reply = c_forbidden()(state("--- "));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dash_followed_by_newline() {
        let reply = c_forbidden()(state("---\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dash_at_eof() {
        let reply = c_forbidden()(state("---"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dot_followed_by_space() {
        let reply = c_forbidden()(state("... "));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dot_followed_by_newline() {
        let reply = c_forbidden()(state("...\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_triggers_on_triple_dot_at_eof() {
        let reply = c_forbidden()(state("..."));
        assert!(is_success(&reply));
    }

    #[test]
    fn c_forbidden_fails_on_triple_dash_followed_by_letter() {
        let reply = c_forbidden()(state("---a"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn c_forbidden_fails_when_not_at_column_zero() {
        // Simulate being at column 1 by using state_with.
        let s = State {
            input: "---\n",
            pos: crate::pos::Pos {
                byte_offset: 1,
                char_offset: 1,
                line: 1,
                column: 1,
            },
            n: 0,
            c: Context::BlockOut,
        };
        let reply = c_forbidden()(s);
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 3: l_document_prefix [204]
    // -----------------------------------------------------------------------

    #[test]
    fn l_document_prefix_succeeds_on_empty_input() {
        let reply = l_document_prefix()(state(""));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_document_prefix_consumes_bom() {
        let reply = l_document_prefix()(state("\u{FEFF}---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    #[test]
    fn l_document_prefix_consumes_comment_lines() {
        let reply = l_document_prefix()(state("# comment\n---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    #[test]
    fn l_document_prefix_consumes_blank_lines() {
        let reply = l_document_prefix()(state("\n\n---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    #[test]
    fn l_document_prefix_consumes_bom_then_comment() {
        let reply = l_document_prefix()(state("\u{FEFF}# comment\n---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    // -----------------------------------------------------------------------
    // Group 4: l_bare_document [206]
    // -----------------------------------------------------------------------

    #[test]
    fn l_bare_document_parses_scalar() {
        let reply = l_bare_document()(state("hello\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_bare_document_emits_begin_and_end_document() {
        let reply = l_bare_document()(state("hello\n"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginDocument));
        assert!(cs.contains(&Code::EndDocument));
    }

    #[test]
    fn l_bare_document_stops_before_directives_end() {
        let reply = l_bare_document()(state("hello\n---\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "---\n");
    }

    #[test]
    fn l_bare_document_stops_before_document_end() {
        let reply = l_bare_document()(state("hello\n...\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "...\n");
    }

    #[test]
    fn l_bare_document_parses_mapping() {
        let reply = l_bare_document()(state("key: value\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginMapping));
    }

    #[test]
    fn l_bare_document_parses_sequence() {
        let reply = l_bare_document()(state("- item\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginSequence));
    }

    #[test]
    fn l_bare_document_fails_when_starts_with_directives_end() {
        let reply = l_bare_document()(state("---\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_bare_document_fails_when_starts_with_document_end() {
        let reply = l_bare_document()(state("...\n"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 5: l_explicit_document [207]
    // -----------------------------------------------------------------------

    #[test]
    fn l_explicit_document_parses_empty_after_marker() {
        // `---\n` with no content — empty explicit document.
        let reply = l_explicit_document()(state("---\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_explicit_document_emits_begin_and_end_document() {
        let reply = l_explicit_document()(state("---\n"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginDocument));
        assert!(cs.contains(&Code::EndDocument));
    }

    #[test]
    fn l_explicit_document_emits_directives_end_token() {
        let reply = l_explicit_document()(state("---\n"));
        assert!(has_code(&reply, Code::DirectivesEnd));
    }

    #[test]
    fn l_explicit_document_parses_scalar_content() {
        let reply = l_explicit_document()(state("--- hello\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_explicit_document_parses_mapping_content() {
        let reply = l_explicit_document()(state("---\nkey: value\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginMapping));
    }

    #[test]
    fn l_explicit_document_leaves_end_marker() {
        let reply = l_explicit_document()(state("---\nhello\n...\n"));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "...\n");
    }

    #[test]
    fn l_explicit_document_fails_without_triple_dash() {
        let reply = l_explicit_document()(state("hello\n"));
        assert!(is_failure(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 6: l_directive_document [208]
    // -----------------------------------------------------------------------

    #[test]
    fn l_directive_document_parses_yaml_directive() {
        let reply = l_directive_document()(state("%YAML 1.2\n---\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_document_emits_begin_and_end_document() {
        let reply = l_directive_document()(state("%YAML 1.2\n---\n"));
        let cs = codes(reply);
        assert!(cs.contains(&Code::BeginDocument));
        assert!(cs.contains(&Code::EndDocument));
    }

    #[test]
    fn l_directive_document_emits_directives_end_token() {
        let reply = l_directive_document()(state("%YAML 1.2\n---\n"));
        assert!(has_code(&reply, Code::DirectivesEnd));
    }

    #[test]
    fn l_directive_document_parses_tag_directive() {
        let reply = l_directive_document()(state("%TAG ! tag:example.com,2024:\n---\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_document_requires_directives_end_after_directive() {
        // A directive without `---` must fail.
        let reply = l_directive_document()(state("%YAML 1.2\nhello\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_directive_document_parses_content_after_directive() {
        let reply = l_directive_document()(state("%YAML 1.2\n---\nhello\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_directive_document_fails_without_any_directive() {
        let reply = l_directive_document()(state("---\nhello\n"));
        assert!(is_failure(&reply));
    }

    #[test]
    fn l_directive_document_parses_multiple_directives() {
        let reply = l_directive_document()(state("%YAML 1.2\n%TAG ! tag:example.com,2024:\n---\n"));
        assert!(is_success(&reply));
    }

    // -----------------------------------------------------------------------
    // Group 7: l_yaml_stream [211]
    // -----------------------------------------------------------------------

    #[test]
    fn l_yaml_stream_accepts_empty_input() {
        let reply = l_yaml_stream()(state(""));
        assert!(is_success(&reply));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_yaml_stream_accepts_single_bare_document() {
        let reply = l_yaml_stream()(state("hello\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_comment_only() {
        let reply = l_yaml_stream()(state("# just a comment\n"));
        assert!(is_success(&reply));
    }

    #[test]
    fn l_yaml_stream_accepts_explicit_document() {
        let reply = l_yaml_stream()(state("---\nhello\n...\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_two_bare_documents() {
        let reply = l_yaml_stream()(state("first\n---\nsecond\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_accepts_two_explicit_documents() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n---\nsecond\n...\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_accepts_bare_then_explicit_document() {
        let reply = l_yaml_stream()(state("first\n---\nsecond\n...\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_accepts_directive_document() {
        let reply = l_yaml_stream()(state("%YAML 1.2\n---\nhello\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_leading_comments_then_document() {
        let reply = l_yaml_stream()(state("# comment\n---\nhello\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_bom_prefix() {
        let reply = l_yaml_stream()(state("\u{FEFF}---\nhello\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginDocument));
    }

    #[test]
    fn l_yaml_stream_accepts_mapping_document() {
        let reply = l_yaml_stream()(state("key: value\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginMapping));
    }

    #[test]
    fn l_yaml_stream_accepts_sequence_document() {
        let reply = l_yaml_stream()(state("- item\n- item2\n"));
        assert!(is_success(&reply));
        assert!(has_code(&reply, Code::BeginSequence));
    }

    #[test]
    fn l_yaml_stream_explicit_document_end_tokens_count() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n---\nsecond\n...\n"));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::EndDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_document_end_markers_count() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n---\nsecond\n...\n"));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::DocumentEnd).count(), 2);
    }

    #[test]
    fn l_yaml_stream_consumes_all_input_for_multi_document() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n---\nsecond\n...\n"));
        assert_eq!(remaining(&reply), "");
    }

    #[test]
    fn l_yaml_stream_empty_explicit_documents() {
        let reply = l_yaml_stream()(state("---\n---\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    #[test]
    fn l_yaml_stream_three_documents() {
        let reply = l_yaml_stream()(state("a\n---\nb\n---\nc\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 3);
    }

    #[test]
    fn l_yaml_stream_blank_lines_between_documents() {
        let reply = l_yaml_stream()(state("---\nfirst\n...\n\n---\nsecond\n...\n"));
        assert!(is_success(&reply));
        let cs = codes(reply);
        assert_eq!(cs.iter().filter(|&&c| c == Code::BeginDocument).count(), 2);
    }

    // -----------------------------------------------------------------------
    // Group 8: tokenize public entry point
    // -----------------------------------------------------------------------

    #[test]
    fn tokenize_empty_input_returns_empty_vec() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn tokenize_simple_mapping_returns_tokens() {
        let tokens = tokenize("key: value\n");
        assert!(!tokens.is_empty());
    }

    #[test]
    fn tokenize_returns_begin_document_token() {
        let tokens = tokenize("key: value\n");
        assert!(tokens.iter().any(|t| t.code == Code::BeginDocument));
    }

    #[test]
    fn tokenize_returns_end_document_token() {
        let tokens = tokenize("key: value\n");
        assert!(tokens.iter().any(|t| t.code == Code::EndDocument));
    }

    #[test]
    fn tokenize_explicit_stream_has_directives_end_token() {
        let tokens = tokenize("---\nhello\n");
        assert!(tokens.iter().any(|t| t.code == Code::DirectivesEnd));
    }

    #[test]
    fn tokenize_multi_doc_stream_has_two_begin_document_tokens() {
        let tokens = tokenize("---\nfirst\n...\n---\nsecond\n...\n");
        assert_eq!(
            tokens
                .iter()
                .filter(|t| t.code == Code::BeginDocument)
                .count(),
            2
        );
    }

    #[test]
    fn tokenize_all_tokens_have_non_empty_or_marker_codes() {
        // Verify no token has Code::Error.
        let tokens = tokenize("key: value\n");
        assert!(!tokens.iter().any(|t| t.code == Code::Error));
    }
}
