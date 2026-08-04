//! Parsers for the ASCII payloads: instrument responses (`KEY=VALUE`, ranges) and
//! command lines. The command-line parser is the inverse of
//! [`crate::spark::commands`] (used by the simulator).

use crate::spark::commands::Op;
use std::collections::HashMap;

/// Parse a Ready payload into ordered `KEY=VALUE` pairs.
///
/// Values are matched by `\b(\w+)=(.*?)(?=\s\w+=\s*|$)` — a value runs until the next
/// ` KEY=` or end of string, so values may contain spaces (and quotes). We scan
/// for `KEY=` starts where KEY is `\w+` preceded by start/whitespace.
pub fn parse_kv(payload: &str) -> Vec<(String, String)> {
    let bytes = payload.as_bytes();
    // Find each key start: an index where a run of word chars is followed by '='
    // and the char before the run is start-of-string or whitespace.
    let is_word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut starts: Vec<(usize, usize, usize)> = Vec::new(); // (key_start, eq_pos, val_start)
    let mut i = 0;
    while i < bytes.len() {
        let at_boundary = i == 0 || (bytes[i - 1] as char).is_whitespace();
        if at_boundary && is_word(bytes[i]) {
            let ks = i;
            let mut j = i;
            while j < bytes.len() && is_word(bytes[j]) {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                starts.push((ks, j, j + 1));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    let mut out = Vec::new();
    for (idx, &(ks, eq, vs)) in starts.iter().enumerate() {
        let key = payload[ks..eq].to_string();
        let val_end = starts
            .get(idx + 1)
            .map(|&(nks, _, _)| nks)
            .unwrap_or(payload.len());
        let val = payload[vs..val_end].trim();
        let val = val
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(val);
        out.push((key, val.to_string()));
    }
    out
}

/// Same as [`parse_kv`] but as a map (last value wins on duplicate keys).
pub fn parse_kv_map(payload: &str) -> HashMap<String, String> {
    parse_kv(payload).into_iter().collect()
}

/// A parsed range definition — `{from}~{to}%{step} [unit]` (count-step) or
/// `{from}~{to}:{step} [unit]` (delta-step).
#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    pub from: f64,
    pub to: f64,
    pub step: Option<f64>,
    /// true => `:` delta step; false => `%` count step; None => no step.
    pub is_delta: Option<bool>,
    pub unit: Option<String>,
}

/// Parse a range like `234~894%2 [ang]` or `24~42:0.1 [c100]`.
pub fn parse_range(s: &str) -> Option<Range> {
    let s = s.trim();
    // Split off a trailing `[unit]`.
    let (body, unit) = match (s.rfind('['), s.rfind(']')) {
        (Some(a), Some(b)) if b > a => (s[..a].trim(), Some(s[a + 1..b].trim().to_string())),
        _ => (s, None),
    };
    let (from_s, rest) = body.split_once('~')?;
    // rest may carry a step introduced by '%' (count) or ':' (delta).
    let (to_s, step, is_delta) = if let Some((t, st)) = rest.split_once('%') {
        (t, st.trim().parse().ok(), Some(false))
    } else if let Some((t, st)) = rest.split_once(':') {
        (t, st.trim().parse().ok(), Some(true))
    } else {
        (rest, None, None)
    };
    Some(Range {
        from: from_s.trim().parse().ok()?,
        to: to_s.trim().parse().ok()?,
        step,
        is_delta,
        unit,
    })
}

/// A parsed command line (inverse of [`crate::spark::commands::Command`]) — used by the
/// simulator to dispatch.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCommand {
    pub op: Op,
    pub keyword: String,
    /// Bare words after the keyword that are not `KEY=VALUE` (e.g. `START`).
    pub words: Vec<String>,
    pub params: Vec<(String, String)>,
    pub module: Option<u32>,
    pub number: Option<u32>,
    pub sub: Option<u32>,
}

/// Parse `[prefix]KEYWORD [WORD] [KEY=VALUE...] [MODULE=n NUMBER=n SUB=n]`.
/// Splits on whitespace but keeps `"quoted values"` together.
pub fn parse_command_line(line: &str) -> Option<ParsedCommand> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (op, rest) = match line.as_bytes()[0] {
        b'#' => (Op::Range, &line[1..]),
        b'?' => (Op::Query, &line[1..]),
        _ => (Op::Set, line),
    };
    let toks = tokenize(rest);
    let mut it = toks.into_iter();
    let keyword = it.next()?;
    let mut cmd = ParsedCommand {
        op,
        keyword,
        words: Vec::new(),
        params: Vec::new(),
        module: None,
        number: None,
        sub: None,
    };
    for tok in it {
        if let Some((k, v)) = tok.split_once('=') {
            match k {
                "MODULE" => cmd.module = v.parse().ok(),
                "NUMBER" => cmd.number = v.parse().ok(),
                "SUB" => cmd.sub = v.parse().ok(),
                _ => cmd.params.push((k.to_string(), unquote(v))),
            }
        } else {
            cmd.words.push(tok);
        }
    }
    Some(cmd)
}

fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut has = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                cur.push('"');
                has = true;
            }
            c if c.is_whitespace() && !in_q => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

fn unquote(v: &str) -> String {
    v.strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(v)
        .to_string()
}
