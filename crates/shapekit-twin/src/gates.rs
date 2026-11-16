//! Static list of stellar-core's protocol gates: every call of
//! `protocolVersionIsBefore`, `protocolVersionStartsFrom` and
//! `protocolVersionEquals` (`src/util/ProtocolVersion.h`) in core's non-test
//! C++ source, with the function it sits in and the protocol it compares
//! against.
//!
//! The scanner is a tokenizer, not a compiler: it removes comments, string
//! and character literals and preprocessor lines, then follows braces to know
//! whether a call sits in a function body and which one. That is enough for
//! core's source, and [`scan_tree`] fails instead of guessing when a call's
//! enclosing function or argument list cannot be read. Calls at namespace or
//! class scope are the declarations and definitions of the three functions
//! themselves; they are counted separately and are not gates.
//!
//! Left out, like the rest of the twin's denominator: test directories, the
//! XDR definitions (`protocol-curr`), the vendored contract hosts
//! (`rust/soroban`) and the load generator (`simulation`).

use std::collections::BTreeMap;
use std::path::Path;

pub use shapekit_contracts::gates::Comparison;

/// The three gate functions and the comparison each makes.
pub const GATE_FUNCTIONS: [(&str, Comparison); 3] = [
    ("protocolVersionIsBefore", Comparison::IsBefore),
    ("protocolVersionStartsFrom", Comparison::StartsFrom),
    ("protocolVersionEquals", Comparison::Equals),
];

/// Directories under `src/` that are not part of the measured code.
pub const EXCLUDED_DIRS: [&str; 4] = ["test", "protocol-curr", "simulation", "soroban"];

/// One gate call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateCall {
    /// Path from the core checkout's root, `src/...`.
    pub file: String,
    pub line: u32,
    /// 1-based column of the call, in characters.
    pub column: u32,
    /// Enclosing function as written at its definition, with the class for
    /// member functions defined inside the class body.
    pub function: String,
    pub comparison: Comparison,
    /// Source text of the version argument (the first).
    pub version_arg: String,
    /// Source text of the protocol argument (the second).
    pub protocol_arg: String,
    /// The protocol the second argument names, when it is a constant.
    pub protocol: Option<u32>,
    /// The call is only compiled with `BUILD_TESTS`.
    pub build_tests_only: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum GatesError {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{file}:{line}: {what}")]
    Parse {
        file: String,
        line: u32,
        what: String,
    },
}

/// A function body in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSpan {
    pub file: String,
    pub function: String,
    pub line_start: u32,
    pub line_end: u32,
}

/// The gates of a tree plus what was counted but is not a gate.
#[derive(Debug, Clone, Default)]
pub struct GateScan {
    pub gates: Vec<GateCall>,
    /// Every function body, with the lines it spans.
    pub functions: Vec<FunctionSpan>,
    /// Per file, the lines holding code compiled only with `BUILD_TESTS`.
    pub tests_only_lines: BTreeMap<String, std::collections::BTreeSet<u32>>,
    /// Matches outside any function body: the declarations and definitions
    /// of the gate functions.
    pub declarations: Vec<(String, u32)>,
    pub files_scanned: usize,
}

/// Scans `<core_src>/src`. Files are visited in path order so the output is
/// stable.
pub fn scan_tree(core_src: &Path) -> Result<GateScan, GatesError> {
    let root = core_src.join("src");
    let mut files = Vec::new();
    collect(&root, &root, &mut files)?;
    files.sort();
    let mut texts = Vec::with_capacity(files.len());
    for rel in &files {
        let path = root.join(rel);
        let text = std::fs::read_to_string(&path).map_err(|source| GatesError::Io {
            path: path.display().to_string(),
            source,
        })?;
        texts.push((format!("src/{rel}"), text));
    }
    let mut constants = BTreeMap::new();
    for (_, t) in &texts {
        collect_constants(t, &mut constants);
    }
    let mut scan = GateScan {
        files_scanned: texts.len(),
        ..GateScan::default()
    };
    for (file, text) in &texts {
        scan_file(file, text, &constants, &mut scan)?;
    }
    Ok(scan)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), GatesError> {
    let io = |source| GatesError::Io {
        path: dir.display().to_string(),
        source,
    };
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().map_err(io)?.is_dir() {
            if EXCLUDED_DIRS.contains(&name.as_str()) || name.starts_with("test") {
                continue;
            }
            collect(root, &path, out)?;
        } else if name.ends_with(".cpp") || name.ends_with(".h") {
            let rel = path
                .strip_prefix(root)
                .expect("walked from root")
                .to_string_lossy()
                .into_owned();
            out.push(rel);
        }
    }
    Ok(())
}

/// Named protocol constants: `NAME = ProtocolVersion::V_n`, the brace form,
/// and `#define NAME ProtocolVersion::V_n`.
pub fn collect_constants(text: &str, out: &mut BTreeMap<String, u32>) {
    let toks = tokenize(text);
    for w in toks.windows(6) {
        let [name, eq, ns, colons, v, _] = w else {
            continue;
        };
        if name.kind == Kind::Ident
            && (eq.text == "=" || eq.text == "{")
            && ns.text == "ProtocolVersion"
            && colons.text == "::"
            && let Some(n) = version_number(&v.text)
        {
            out.insert(name.text.clone(), n);
        }
    }
    for line in text.lines() {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix('#') {
            let mut it = rest.split_whitespace();
            if it.next() == Some("define")
                && let (Some(name), Some(val)) = (it.next(), it.next())
                && let Some(v) = val.strip_prefix("ProtocolVersion::")
                && let Some(n) = version_number(v)
            {
                out.insert(name.to_owned(), n);
            }
        }
    }
}

fn version_number(s: &str) -> Option<u32> {
    s.strip_prefix("V_")?.parse().ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ident,
    Number,
    Punct,
}

#[derive(Debug, Clone)]
struct Tok {
    kind: Kind,
    text: String,
    line: u32,
    /// Byte range in the source.
    start: usize,
    end: usize,
    /// Inside `#ifdef BUILD_TESTS` (or `#if defined(BUILD_TESTS)`).
    tests_only: bool,
}

/// Tokens of C++ source without comments, literals and preprocessor lines.
/// String and character literals become a single `"` punctuation token so
/// argument text keeps its shape.
fn tokenize(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1u32;
    let mut at_line_start = true;
    // stack of #if frames: Some(true) = BUILD_TESTS branch, Some(false) = its #else, None = other
    let mut pp: Vec<Option<bool>> = Vec::new();
    let tests_only = |pp: &Vec<Option<bool>>| pp.contains(&Some(true));
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line += 1;
            at_line_start = true;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if at_line_start && c == b'#' {
            // preprocessor line, with backslash continuations
            let start = i;
            while i < b.len() && !(b[i] == b'\n' && b[i - 1] != b'\\') {
                if b[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            let directive: String = src[start + 1..i]
                .replace("\\\n", " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if directive.starts_with("ifdef BUILD_TESTS")
                || directive.starts_with("if defined(BUILD_TESTS)")
                || directive == "if BUILD_TESTS"
            {
                pp.push(Some(true));
            } else if directive.starts_with("ifndef BUILD_TESTS")
                || directive.starts_with("if !defined(BUILD_TESTS)")
            {
                pp.push(Some(false));
            } else if directive.starts_with("if") {
                pp.push(None);
            } else if directive.starts_with("else") || directive.starts_with("elif") {
                if let Some(Some(f)) = pp.last_mut() {
                    *f = !*f;
                }
            } else if directive.starts_with("endif") {
                pp.pop();
            }
            continue;
        }
        at_line_start = false;
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                if b[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        let start = i;
        let start_line = line;
        // raw string R"delim( ... )delim", possibly with an encoding prefix
        if c == b'R' && b.get(i + 1) == Some(&b'"') {
            let open = i + 2;
            let paren = src[open..].find('(').map(|p| open + p).unwrap_or(b.len());
            let delim = &src[open..paren];
            let close = format!("){delim}\"");
            let endpos = src[paren..]
                .find(&close)
                .map(|p| paren + p + close.len())
                .unwrap_or(b.len());
            line += src[start..endpos].matches('\n').count() as u32;
            i = endpos;
            out.push(Tok {
                kind: Kind::Punct,
                text: "\"".into(),
                line: start_line,
                start,
                end: i,
                tests_only: tests_only(&pp),
            });
            continue;
        }
        if c == b'"' || c == b'\'' {
            i += 1;
            while i < b.len() && b[i] != c {
                if b[i] == b'\\' {
                    i += 1;
                }
                if i < b.len() && b[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 1;
            out.push(Tok {
                kind: Kind::Punct,
                text: "\"".into(),
                line: start_line,
                start,
                end: i.min(b.len()),
                tests_only: tests_only(&pp),
            });
            continue;
        }
        let (kind, len) = if c.is_ascii_alphabetic() || c == b'_' {
            let n = b[i..]
                .iter()
                .take_while(|x| x.is_ascii_alphanumeric() || **x == b'_')
                .count();
            (Kind::Ident, n)
        } else if c.is_ascii_digit() {
            let n = b[i..]
                .iter()
                .take_while(|x| x.is_ascii_alphanumeric() || **x == b'.' || **x == b'\'')
                .count();
            (Kind::Number, n)
        } else if b[i..].starts_with(b"::") || b[i..].starts_with(b"->") {
            (Kind::Punct, 2)
        } else {
            // one character, which outside ascii (a byte order mark, a
            // character in a macro body) spans several bytes
            (
                Kind::Punct,
                src[i..].chars().next().map_or(1, char::len_utf8),
            )
        };
        i += len;
        out.push(Tok {
            kind,
            text: src[start..i].to_owned(),
            line: start_line,
            start,
            end: i,
            tests_only: tests_only(&pp),
        });
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Scope {
    Namespace,
    Class(String),
    Function(String),
    /// Braces inside a function body.
    Block,
    /// Braces of a member initializer in a constructor's init list.
    InitBrace,
    /// Braces inside an unfinished expression outside a function body, such
    /// as a lambda in a member initializer. Calls inside count for the
    /// function whose header is being read.
    ExprBrace(String),
    /// Any other braces outside a function: aggregate initializers, enums.
    Other,
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parentheses and brackets opened in `h` and not closed.
fn open_parens(h: &[Tok]) -> i32 {
    h.iter()
        .map(|t| match t.text.as_str() {
            "(" | "[" => 1,
            ")" | "]" => -1,
            _ => 0,
        })
        .sum()
}

/// Index of the first `(` at angle and paren depth 0 in `h`.
fn first_call_paren(h: &[Tok]) -> Option<usize> {
    let mut angle = 0i32;
    for (i, t) in h.iter().enumerate() {
        match t.text.as_str() {
            "<" => {
                // `operator<` is a name, not a template bracket
                if i == 0 || h[i - 1].text != "operator" {
                    angle += 1;
                }
            }
            ">" if angle > 0 => angle -= 1,
            "(" if angle == 0 => {
                // `operator()(...)`: the first pair is the name
                if i > 0 && h[i - 1].text == "operator" {
                    continue;
                }
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

/// Index of the `<` matching the `>` at `h[close]`.
fn skip_template_args(h: &[Tok], close: usize) -> Option<usize> {
    let mut depth = 0;
    let mut k = close;
    loop {
        match h[k].text.as_str() {
            ">" => depth += 1,
            "<" => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
        if k == 0 {
            return None;
        }
        k -= 1;
    }
}

/// Function name ending right before `h[paren]`.
fn function_name(src: &str, h: &[Tok], paren: usize) -> Option<String> {
    let mut j = paren;
    // operator names: walk back to `operator`
    if let Some(op) = h[..paren].iter().rposition(|t| t.text == "operator")
        && paren - op <= 4
    {
        j = op;
    } else {
        // explicit template arguments: name<...>(
        if j >= 1 && h[j - 1].text == ">" {
            j = skip_template_args(h, j - 1)?;
        }
        if j == 0 || h[j - 1].kind != Kind::Ident {
            return None;
        }
        j -= 1;
    }
    // qualifiers: Name:: and ~ and template arguments Name<...>::
    loop {
        if j >= 1 && h[j - 1].text == "~" {
            j -= 1;
            continue;
        }
        if j >= 2 && h[j - 1].text == "::" && h[j - 2].kind == Kind::Ident {
            j -= 2;
            continue;
        }
        if j >= 2 && h[j - 1].text == "::" && h[j - 2].text == ">" {
            let k = skip_template_args(h, j - 2)?;
            if k >= 1 && h[k - 1].kind == Kind::Ident {
                j = k - 1;
                continue;
            }
            return None;
        }
        break;
    }
    Some(collapse(&src[h[j].start..h[paren - 1].end]))
}

/// The header has a parameter list followed by a `:`, a constructor's
/// member initializer list.
fn is_init_list(h: &[Tok]) -> bool {
    let Some(paren) = first_call_paren(h) else {
        return false;
    };
    let mut depth = 0;
    for (i, t) in h.iter().enumerate().skip(paren) {
        match t.text.as_str() {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 {
                    return h[i + 1..].iter().any(|t| t.text == ":");
                }
            }
            _ => {}
        }
    }
    false
}

/// The header looks like a constructor with a member init list and the
/// brace about to open belongs to a member initializer.
fn is_init_brace(h: &[Tok]) -> bool {
    let Some(paren) = first_call_paren(h) else {
        return false;
    };
    // find the parameter list's close, then a ':' after it
    let mut depth = 0;
    let mut close = None;
    for (i, t) in h.iter().enumerate().skip(paren) {
        match t.text.as_str() {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else { return false };
    let colon = h[close + 1..].iter().any(|t| t.text == ":");
    let last = h.last().expect("non-empty");
    colon && (last.kind == Kind::Ident || last.text == ">")
}

fn classify(src: &str, h: &[Tok], file: &str) -> Result<Scope, GatesError> {
    let first_real = h
        .iter()
        .position(|t| t.text != "template" && t.text != "typedef")
        .map(|i| &h[i..])
        .unwrap_or(&[]);
    if h.iter().any(|t| t.text == "namespace")
        || (h.len() >= 2 && h[0].text == "extern" && h[1].text == "\"")
    {
        return Ok(Scope::Namespace);
    }
    // template<...> class X
    let skip_template = {
        let mut i = 0;
        if h.first().map(|t| t.text.as_str()) == Some("template") {
            let mut depth = 0;
            for (k, t) in h.iter().enumerate() {
                match t.text.as_str() {
                    "<" => depth += 1,
                    ">" => {
                        depth -= 1;
                        if depth == 0 {
                            i = k + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        &h[i..]
    };
    let _ = first_real;
    if let Some(t) = skip_template.first()
        && matches!(t.text.as_str(), "class" | "struct" | "union" | "enum")
        && first_call_paren(skip_template).is_none_or(|p| {
            // a struct with a parenthesised attribute or base is still a class
            skip_template[..p].iter().any(|x| x.text == ":")
        })
    {
        let name = skip_template
            .iter()
            .skip(1)
            .take_while(|x| x.text != ":" && x.text != "final")
            .filter(|x| x.kind == Kind::Ident && x.text != "class" && x.text != "struct")
            .last()
            .map(|x| x.text.clone())
            .unwrap_or_default();
        return Ok(Scope::Class(name));
    }
    let top_eq = {
        let mut depth = 0;
        let mut found = false;
        for t in h {
            match t.text.as_str() {
                "(" | "[" => depth += 1,
                ")" | "]" => depth -= 1,
                "=" if depth == 0 => {
                    found = true;
                    break;
                }
                _ => {}
            }
        }
        found && !h.iter().any(|t| t.text == "operator")
    };
    if top_eq {
        // `T x = {` aggregate, or `auto f = [](...) {` lambda at file scope
        if h.iter().any(|t| t.text == "[") && h.last().is_some_and(|t| t.text != "=") {
            let name = h
                .iter()
                .take_while(|t| t.text != "=")
                .filter(|t| t.kind == Kind::Ident)
                .last()
                .map(|t| t.text.clone())
                .unwrap_or_default();
            return Ok(Scope::Function(name));
        }
        return Ok(Scope::Other);
    }
    if let Some(p) = first_call_paren(h) {
        return match function_name(src, h, p) {
            Some(n) => Ok(Scope::Function(n)),
            None => Err(GatesError::Parse {
                file: file.to_owned(),
                line: h[p].line,
                what: format!(
                    "cannot read the function name in `{}`",
                    collapse(&src[h[0].start..h[h.len() - 1].end])
                ),
            }),
        };
    }
    Ok(Scope::Other)
}

/// Adds the gates and declarations of one file to `scan`.
pub fn scan_file(
    file: &str,
    src: &str,
    constants: &BTreeMap<String, u32>,
    scan: &mut GateScan,
) -> Result<(), GatesError> {
    let toks = tokenize(src);
    let tests_only: std::collections::BTreeSet<u32> = toks
        .iter()
        .filter(|t| t.tests_only)
        .map(|t| t.line)
        .collect();
    if !tests_only.is_empty() {
        scan.tests_only_lines.insert(file.to_owned(), tests_only);
    }
    let mut open_functions: Vec<(String, u32)> = Vec::new();
    let mut scopes: Vec<Scope> = Vec::new();
    let mut header_start = 0usize;
    let mut i = 0;
    let function = |scopes: &Vec<Scope>| -> Option<String> {
        let at = scopes
            .iter()
            .rposition(|s| matches!(s, Scope::Function(_) | Scope::ExprBrace(_)))?;
        let (Scope::Function(name) | Scope::ExprBrace(name)) = &scopes[at] else {
            unreachable!()
        };
        let class = scopes[..at].iter().rev().find_map(|s| match s {
            Scope::Class(c) if !c.is_empty() => Some(c.clone()),
            _ => None,
        });
        Some(match class {
            Some(c) if !name.contains("::") => format!("{c}::{name}"),
            _ => name.clone(),
        })
    };
    while i < toks.len() {
        let t = &toks[i];
        match t.text.as_str() {
            "{" => {
                let in_fn = scopes
                    .iter()
                    .any(|s| matches!(s, Scope::Function(_) | Scope::ExprBrace(_)));
                let scope = if in_fn {
                    Scope::Block
                } else {
                    let h = &toks[header_start..i];
                    if open_parens(h) > 0 {
                        let name = first_call_paren(h)
                            .and_then(|p| function_name(src, h, p))
                            .ok_or_else(|| GatesError::Parse {
                                file: file.to_owned(),
                                line: t.line,
                                what: "brace inside an expression with no function header".into(),
                            })?;
                        Scope::ExprBrace(name)
                    } else if !h.is_empty() && is_init_brace(h) {
                        Scope::InitBrace
                    } else if h.is_empty() {
                        Scope::Other
                    } else {
                        classify(src, h, file)?
                    }
                };
                let init = matches!(scope, Scope::InitBrace | Scope::ExprBrace(_));
                let opens_function = matches!(scope, Scope::Function(_));
                scopes.push(scope);
                if opens_function {
                    let name = function(&scopes).expect("a function scope was pushed");
                    open_functions.push((name, t.line));
                }
                if !init {
                    header_start = i + 1;
                }
            }
            "}" => {
                let popped = scopes.pop().ok_or_else(|| GatesError::Parse {
                    file: file.to_owned(),
                    line: t.line,
                    what: "unbalanced closing brace".into(),
                })?;
                if matches!(popped, Scope::Function(_))
                    && let Some((name, start)) = open_functions.pop()
                {
                    scan.functions.push(FunctionSpan {
                        file: file.to_owned(),
                        function: name,
                        line_start: start,
                        line_end: t.line,
                    });
                }
                // inside an expression brace the enclosing header is still
                // being read, whatever closes within it
                if !matches!(popped, Scope::InitBrace | Scope::ExprBrace(_))
                    && !scopes.iter().any(|s| matches!(s, Scope::ExprBrace(_)))
                {
                    header_start = i + 1;
                }
            }
            ";" => {
                if !scopes
                    .iter()
                    .any(|s| matches!(s, Scope::Function(_) | Scope::ExprBrace(_)))
                {
                    header_start = i + 1;
                }
            }
            name if t.kind == Kind::Ident => {
                if let Some((_, cmp)) = GATE_FUNCTIONS.iter().find(|(f, _)| *f == name)
                    && toks.get(i + 1).is_some_and(|n| n.text == "(")
                {
                    let (args, close) =
                        call_args(src, &toks, i + 1).ok_or_else(|| GatesError::Parse {
                            file: file.to_owned(),
                            line: t.line,
                            what: format!("unterminated call of {name}"),
                        })?;
                    // outside a body, a call still belongs to a function when
                    // it sits in that function's member initializer list
                    let owner = function(&scopes).or_else(|| {
                        let h = &toks[header_start..i];
                        let p = first_call_paren(h)?;
                        let name = function_name(src, h, p)?;
                        (open_parens(h) > 0 || is_init_list(h)).then_some(name)
                    });
                    match owner {
                        None => scan.declarations.push((file.to_owned(), t.line)),
                        Some(f) => {
                            if args.len() != 2 {
                                return Err(GatesError::Parse {
                                    file: file.to_owned(),
                                    line: t.line,
                                    what: format!("{name} called with {} arguments", args.len()),
                                });
                            }
                            let line_start = src[..t.start].rfind('\n').map_or(0, |p| p + 1);
                            scan.gates.push(GateCall {
                                file: file.to_owned(),
                                line: t.line,
                                column: src[line_start..t.start].chars().count() as u32 + 1,
                                function: f,
                                comparison: *cmp,
                                version_arg: args[0].clone(),
                                protocol: resolve(&args[1], constants),
                                protocol_arg: args[1].clone(),
                                build_tests_only: t.tests_only,
                            });
                        }
                    }
                    i = close;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if !scopes.is_empty() {
        return Err(GatesError::Parse {
            file: file.to_owned(),
            line: toks.last().map_or(0, |t| t.line),
            what: format!("{} braces left open", scopes.len()),
        });
    }
    Ok(())
}

/// Arguments of the call whose `(` is `toks[open]`, as collapsed source
/// text, and the index of the closing `)`.
fn call_args(src: &str, toks: &[Tok], open: usize) -> Option<(Vec<String>, usize)> {
    let mut depth = 0;
    let mut args = Vec::new();
    let mut arg_start = open + 1;
    for (k, t) in toks.iter().enumerate().skip(open) {
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    if k > arg_start {
                        args.push(collapse(&src[toks[arg_start].start..toks[k - 1].end]));
                    }
                    return Some((args, k));
                }
            }
            "," if depth == 1 => {
                args.push(collapse(&src[toks[arg_start].start..toks[k - 1].end]));
                arg_start = k + 1;
            }
            _ => {}
        }
    }
    None
}

fn resolve(arg: &str, constants: &BTreeMap<String, u32>) -> Option<u32> {
    let a = arg.trim();
    let a = a.strip_prefix("ProtocolVersion::").unwrap_or(a);
    // a class constant is collected under its own name
    let last = a.rsplit("::").next().unwrap_or(a).trim();
    version_number(a).or_else(|| constants.get(last).copied())
}

/// The gate list contract over a scan: every call comparing against a
/// protocol constant, both sides open until something witnesses them. Calls
/// comparing against a runtime value have no protocol to record and are
/// returned apart.
pub fn gate_list(
    scan: &GateScan,
    core: shapekit_contracts::CoreRef,
) -> (shapekit_contracts::gates::GateList, Vec<GateCall>) {
    use shapekit_contracts::gates::{Branch, Gate, GateList, GateSide};
    use shapekit_contracts::obligations::ObligationStatus;
    let open = |branch| GateSide {
        branch,
        status: ObligationStatus::Open,
        witnesses: Vec::new(),
        evidence: None,
    };
    let mut gates = Vec::new();
    let mut runtime = Vec::new();
    for g in &scan.gates {
        match g.protocol {
            Some(version) => gates.push(Gate {
                file: g.file.clone(),
                line: g.line,
                column: g.column,
                function: g.function.clone(),
                comparison: g.comparison,
                version,
                sides: vec![open(Branch::True), open(Branch::False)],
            }),
            None => runtime.push(g.clone()),
        }
    }
    gates.sort_by(|a, b| (&a.file, a.line, a.column).cmp(&(&b.file, b.line, b.column)));
    (
        GateList {
            schema_version: <GateList as shapekit_contracts::Contract>::VERSION,
            core,
            gates,
        },
        runtime,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(src: &str) -> GateScan {
        let mut c = BTreeMap::new();
        collect_constants(src, &mut c);
        let mut s = GateScan::default();
        scan_file("src/x.cpp", src, &c, &mut s).unwrap();
        s
    }

    #[test]
    fn finds_gates_in_functions_and_skips_declarations() {
        let src = r#"
namespace stellar {
bool protocolVersionIsBefore(uint32_t protocolVersion, ProtocolVersion beforeVersion);
constexpr ProtocolVersion SOROBAN_PROTOCOL_VERSION = ProtocolVersion::V_20;
namespace {
// protocolVersionStartsFrom(x, ProtocolVersion::V_1) in a comment
int helper(int v) {
    auto s = "protocolVersionEquals(v, ProtocolVersion::V_3)";
    if (protocolVersionStartsFrom(v,
                                  ProtocolVersion::V_19)) { return 1; }
    auto f = [&](int x) { return protocolVersionIsBefore(x, SOROBAN_PROTOCOL_VERSION); };
    return f(v);
}
}
SetOptionsOpFrame::SetOptionsOpFrame(Operation const& op, TransactionFrame const& parentTx)
    : OperationFrame(op, parentTx), mSetOptions{mOperation.body.setOptionsOp()}
{
#ifdef BUILD_TESTS
    protocolVersionEquals(1, ProtocolVersion::V_10);
#endif
}
class Foo : public Bar {
    bool check(uint32_t v) const override { return protocolVersionEquals(v, minVer); }
};
}
"#;
        let s = scan(src);
        assert_eq!(s.declarations, vec![("src/x.cpp".to_owned(), 3)]);
        let got: Vec<_> = s
            .gates
            .iter()
            .map(|g| {
                (
                    g.line,
                    g.function.as_str(),
                    g.comparison,
                    g.protocol,
                    g.build_tests_only,
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (9, "helper", Comparison::StartsFrom, Some(19), false),
                (11, "helper", Comparison::IsBefore, Some(20), false),
                (
                    19,
                    "SetOptionsOpFrame::SetOptionsOpFrame",
                    Comparison::Equals,
                    Some(10),
                    true
                ),
                (23, "Foo::check", Comparison::Equals, None, false),
            ]
        );
        assert_eq!(s.gates[0].protocol_arg, "ProtocolVersion::V_19");
        assert_eq!(s.gates[3].protocol_arg, "minVer");
    }

    #[test]
    fn the_list_holds_constant_gates_sorted_with_both_sides_open() {
        let src = "namespace s {\nvoid b(uint32_t v) { protocolVersionEquals(v, minVer); }\nvoid a(uint32_t v) { if (protocolVersionIsBefore(v, ProtocolVersion::V_19) && protocolVersionStartsFrom(v, ProtocolVersion::V_3)) {} }\n}\n";
        let s = scan(src);
        let core = shapekit_contracts::CoreRef {
            version: "v30.0.0".into(),
            commit: "0e49751aa9ddf892302cca02cea7f3f5d97df9f1".into(),
            image: None,
        };
        let (list, runtime) = gate_list(&s, core);
        assert_eq!(runtime.len(), 1);
        assert_eq!(runtime[0].protocol_arg, "minVer");
        let got: Vec<_> = list
            .gates
            .iter()
            .map(|g| (g.line, g.column, g.version))
            .collect();
        assert_eq!(got, vec![(3, 26, 19), (3, 79, 3)]);
        assert!(shapekit_contracts::Contract::problems(&list).is_empty());
        assert!(list.gates.iter().all(|g| g.sides.len() == 2));
    }

    #[test]
    fn templated_return_types_do_not_hide_the_name() {
        let src = "std::function<void(int)> Foo<T>::make(uint32_t v) { protocolVersionIsBefore(v, ProtocolVersion::V_2); }";
        let s = scan(src);
        assert_eq!(s.gates[0].function, "Foo<T>::make");
    }
}
