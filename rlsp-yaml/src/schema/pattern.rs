// SPDX-License-Identifier: MIT

//! Compiled schema patterns: the limits a pattern is compiled under, the
//! priced size tiers, and the per-thread cache of the outcomes.
//!
//! Patterns come from schemas fetched from remote URLs, so compiling one is
//! work the caller must pay for. The module does not know the caller's budget:
//! every compile attempt is paid through a closure the caller passes in.

use std::cell::RefCell;
use std::collections::HashMap;

use regex::RegexBuilder;

/// Maximum length of a `pattern` string we will compile and match, as a
/// guard against pathological `ReDoS` inputs.
pub const MAX_PATTERN_LEN: usize = 1024;

/// Maximum compiled NFA size for a regex, as a memory guard.
pub const REGEX_SIZE_LIMIT: usize = 512 * 1024;

/// Units of scanning work (comparisons, key lookups, copied nodes, text bytes)
/// that one budget step pays for. The step that enters a node covers the first
/// block of work, so only larger scans cost more.
pub const WORK_PER_STEP: usize = 64;

thread_local! {
    /// Per-thread cache of compiled schema patterns, keyed by pattern string.
    /// Saves the compile, never the price: a run charges what a pattern's
    /// compilation is priced at whether or not it finds it here.
    static REGEX_CACHE: RefCell<HashMap<String, Compiled>> = RefCell::new(HashMap::new());
}

/// Size limits a pattern is compiled under, smallest first, with the base price
/// in budget steps of an attempt under each. A pattern too big for one limit is
/// retried under the next; the last is `REGEX_SIZE_LIMIT`, so which patterns
/// compile is the same as with that limit alone, but an ordinary pattern pays
/// for the small attempt that suffices rather than for the worst case.
///
/// The base prices cover the compile work up to the limit. Each attempt also
/// parses the pattern, which `REGEX_PARSE_STEPS_PER_BYTE` covers.
///
/// Measured worst attempts, as steps (about 183 ns each), over short (up to 16
/// bytes) pathological patterns such as `(\w{20}){20}`: 4 KiB: ~230; 64 KiB:
/// ~2.0k; 512 KiB: ~16.6k. Parsing a 790-byte alternation took ~0.9k steps at
/// 4 KiB. The base prices are about twice those worst attempts; the per-byte
/// price covers parsing with the same margin. Re-measure before lowering them.
///
/// The third number of a tier is the work units (`WORK_PER_STEP` of them make a
/// step) that matching one byte of haystack costs against a pattern compiled
/// under it. Programs up to 64 KiB match in a small fraction of a step per byte;
/// a program near `REGEX_SIZE_LIMIT` can fall out of the lazy DFA into the NFA
/// simulation, measured up to ~210 steps per byte on a 64 KiB haystack (for
/// example `(a|b)*[a-z]{7276}b` against `aaaa...`), so the last tier is priced
/// at 512 steps per byte.
const COMPILE_TIERS: [(usize, usize, usize); 3] = [
    (4 * 1024, 512, 1),
    (64 * 1024, 4_096, 1),
    (REGEX_SIZE_LIMIT, 32_768, 512 * WORK_PER_STEP),
];

#[cfg(test)]
thread_local! {
    /// Compile attempts made on this thread, for tests that count compilations.
    static COMPILE_ATTEMPTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Compile attempts made on this thread so far.
#[cfg(test)]
pub fn compile_attempts() -> usize {
    COMPILE_ATTEMPTS.with(std::cell::Cell::get)
}

/// Budget steps per byte of pattern for parsing it, charged on every attempt.
const REGEX_PARSE_STEPS_PER_BYTE: usize = 2;

/// The outcome of compiling a pattern and what that was priced at.
#[derive(Clone)]
pub struct Compiled {
    /// `None` when the pattern is not a valid regex within `REGEX_SIZE_LIMIT`.
    pub regex: Option<regex::Regex>,
    /// Budget steps the attempts that led to this outcome were priced at.
    pub steps: usize,
    /// Work units matching one byte of haystack costs, from the tier it compiled
    /// under.
    pub matching_work_per_byte: usize,
}

impl Compiled {
    /// Budget steps for one use of the pattern: one step plus the matching of a
    /// haystack of `haystack_len` bytes, priced per tier. An invalid pattern is
    /// never matched, but a use still costs a step: a schema can list many of
    /// them.
    pub const fn use_steps(&self, haystack_len: usize) -> usize {
        let matching = if self.regex.is_some() {
            haystack_len.saturating_mul(self.matching_work_per_byte)
        } else {
            0
        };
        (matching / WORK_PER_STEP).saturating_add(1)
    }
}

/// Budget steps for one compile attempt of a pattern of `len` bytes under the
/// tier with base price `base`.
const fn attempt_steps(base: usize, len: usize) -> usize {
    base.saturating_add(len.saturating_mul(REGEX_PARSE_STEPS_PER_BYTE))
}

/// Compile `pattern` tier by tier, calling `pay` with each attempt's price
/// before making it. Returns `None` when `pay` refuses an attempt.
///
/// Only a program too big for the tier's limit moves on to the next tier: any
/// other error, a syntax error above all, would recur under every limit.
pub fn compile_priced(pattern: &str, mut pay: impl FnMut(usize) -> bool) -> Option<Compiled> {
    let mut steps = 0_usize;
    for (limit, base, matching_work_per_byte) in COMPILE_TIERS {
        let price = attempt_steps(base, pattern.len());
        if !pay(price) {
            return None;
        }
        steps = steps.saturating_add(price);
        #[cfg(test)]
        COMPILE_ATTEMPTS.with(|attempts| attempts.set(attempts.get() + 1));
        match RegexBuilder::new(pattern).size_limit(limit).build() {
            Ok(regex) => {
                return Some(Compiled {
                    regex: Some(regex),
                    steps,
                    matching_work_per_byte,
                });
            }
            Err(regex::Error::CompiledTooBig(_)) => {}
            Err(_) => {
                return Some(Compiled {
                    regex: None,
                    steps,
                    matching_work_per_byte,
                });
            }
        }
    }
    Some(Compiled {
        regex: None,
        steps,
        matching_work_per_byte: 0,
    })
}

/// Return the outcome of compiling `pattern`, from the per-thread cache when it
/// holds it, otherwise by compiling it and caching the outcome.
///
/// `pay` is called with the steps to pay and whether the outcome came from the
/// cache. A cache hit calls it once with the price of the whole compilation; a
/// miss calls it with the price of each attempt before making it. The price
/// depends only on the pattern, so what a caller pays does not depend on what
/// the cache holds. Returns `None` when `pay` refuses a payment.
pub fn lookup_or_compile(
    pattern: &str,
    mut pay: impl FnMut(usize, bool) -> bool,
) -> Option<Compiled> {
    let cached = REGEX_CACHE.with(|cache| cache.borrow().get(pattern).cloned());
    if let Some(compiled) = cached {
        return pay(compiled.steps, true).then_some(compiled);
    }
    let compiled = compile_priced(pattern, |steps| pay(steps, false))?;
    REGEX_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(pattern.to_string(), compiled.clone());
    });
    Some(compiled)
}
