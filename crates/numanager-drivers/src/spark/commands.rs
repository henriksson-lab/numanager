//! Command-line builder — the ASCII payload of a 0x01 TDCL frame.
//!
//! A command line has the shape:
//! ```text
//! [<prefix>]KEYWORD [SUBKEY] [KEY=VALUE...] [MODULE=<n> [NUMBER=<n>] [SUB=<n>]]
//! ```
//! where the prefix selects the operation.

/// Operation prefix selecting Set/Range/Query semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// No prefix — set / execute an action or write a value.
    Set,
    /// `#` — get definition / allowed range / list of items.
    Range,
    /// `?` — get current value / state.
    Query,
}

impl Op {
    fn prefix(self) -> &'static str {
        match self {
            Op::Set => "",
            Op::Range => "#",
            Op::Query => "?",
        }
    }
}

/// Address of the target module.
/// Empty fields are omitted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Target {
    pub module: Option<u32>,
    pub number: Option<u32>,
    pub sub: Option<u32>,
}

impl Target {
    pub fn module(n: u32) -> Self {
        Target {
            module: Some(n),
            ..Default::default()
        }
    }

    fn append(&self, out: &mut String) {
        // Target order: MODULE, NUMBER, SUB — each a leading-space pair.
        if let Some(m) = self.module {
            out.push_str(&pair("MODULE", &m.to_string()));
        }
        if let Some(n) = self.number {
            out.push_str(&pair("NUMBER", &n.to_string()));
        }
        if let Some(s) = self.sub {
            out.push_str(&pair("SUB", &s.to_string()));
        }
    }
}

/// ` KEY=VALUE` (leading space, `=` separator).
/// Values containing spaces are wrapped in quotes.
fn pair(key: &str, value: &str) -> String {
    format!(" {}={}", key, wrap_if_spaced(value))
}

/// Quote a value only if it contains a space.
fn wrap_if_spaced(v: &str) -> String {
    if v.contains(' ') {
        format!("\"{}\"", v)
    } else {
        v.to_string()
    }
}

/// A command being assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    op: Op,
    keyword: String,
    params: Vec<(String, String)>,
    bare: Vec<String>,
    target: Target,
}

impl Command {
    /// Start a command with an operation and keyword (e.g. `ABSOLUTE`, `INFO`).
    pub fn new(op: Op, keyword: impl Into<String>) -> Self {
        Command {
            op,
            keyword: keyword.into(),
            params: Vec::new(),
            bare: Vec::new(),
            target: Target::default(),
        }
    }

    pub fn set(keyword: impl Into<String>) -> Self {
        Self::new(Op::Set, keyword)
    }
    pub fn query(keyword: impl Into<String>) -> Self {
        Self::new(Op::Query, keyword)
    }
    pub fn range(keyword: impl Into<String>) -> Self {
        Self::new(Op::Range, keyword)
    }

    /// A bare subkeyword with no value (e.g. `START` in `MEASUREMENT START`).
    pub fn word(mut self, w: impl Into<String>) -> Self {
        self.bare.push(w.into());
        self
    }

    /// A `KEY=VALUE` parameter.
    pub fn param(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.params.push((key.into(), value.to_string()));
        self
    }

    /// Address the command at a module (and optional number/sub).
    pub fn target(mut self, t: Target) -> Self {
        self.target = t;
        self
    }

    pub fn module(self, n: u32) -> Self {
        self.target(Target::module(n))
    }

    /// Render the full ASCII command line.
    pub fn build(&self) -> String {
        let mut s = String::new();
        s.push_str(self.op.prefix());
        s.push_str(&self.keyword);
        for w in &self.bare {
            s.push(' ');
            s.push_str(w);
        }
        for (k, v) in &self.params {
            s.push_str(&pair(k, v));
        }
        self.target.append(&mut s);
        s
    }
}
