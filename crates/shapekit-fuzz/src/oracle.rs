//! One input through both decoders, the comparison, and the filter for the known
//! depth-accounting divergence.

use std::panic::{AssertUnwindSafe, catch_unwind};

use anyhow::{Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Map, Value};
use shapekit_conformance::testee::{ErrorClass, Reply, Testee};
use stellar_xdr::Limits;

use crate::types::FuzzType;

/// What one decoder made of one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    /// Decoded the whole input; the re-encoded bytes.
    Accepted(Vec<u8>),
    /// Refused through its normal error path. For rust the error variant, for go its message.
    Rejected(String),
    Panicked(String),
    /// The go testee timed out, died, or answered outside the protocol.
    Failed(String),
}

impl Side {
    pub fn accepted(&self) -> Option<&[u8]> {
        match self {
            Side::Accepted(b) => Some(b),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Side::Accepted(b) => format!("accepts, re-encodes to {} bytes", b.len()),
            Side::Rejected(m) => format!("rejects: {m}"),
            Side::Panicked(m) => format!("panics: {m}"),
            Side::Failed(m) => format!("testee failure: {m}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    RustAcceptsGoRejects,
    GoAcceptsRustRejects,
    ReencodeDiffers,
    RustPanic,
    GoPanic,
    GoFailure,
}

impl Kind {
    pub fn slug(self) -> &'static str {
        match self {
            Kind::RustAcceptsGoRejects => "rust-accepts-go-rejects",
            Kind::GoAcceptsRustRejects => "go-accepts-rust-rejects",
            Kind::ReencodeDiffers => "reencode-differs",
            Kind::RustPanic => "rust-panic",
            Kind::GoPanic => "go-panic",
            Kind::GoFailure => "go-failure",
        }
    }
}

/// The comparison; `None` when the two agree (both reject, or both accept with the same bytes).
pub fn classify(rust: &Side, go: &Side) -> Option<Kind> {
    match (rust, go) {
        (_, Side::Failed(_)) => Some(Kind::GoFailure),
        (Side::Panicked(_), _) => Some(Kind::RustPanic),
        (_, Side::Panicked(_)) => Some(Kind::GoPanic),
        (Side::Rejected(_), Side::Rejected(_)) => None,
        (Side::Accepted(_), Side::Rejected(_)) => Some(Kind::RustAcceptsGoRejects),
        (Side::Rejected(_), Side::Accepted(_)) => Some(Kind::GoAcceptsRustRejects),
        (Side::Accepted(a), Side::Accepted(b)) => (a != b).then_some(Kind::ReencodeDiffers),
        (Side::Failed(_), _) => unreachable!("rust runs in process and never fails this way"),
    }
}

/// The disagreement class an input belongs to: what deduplication, minimisation and the known
/// list key on. Go messages are kept with numbers and quoted values blanked, so one decoder
/// path is one class whatever value tripped it.
pub fn signature(ty: FuzzType, kind: Kind, rust: &Side, go: &Side) -> String {
    format!(
        "{}|{}|rust:{}|go:{}",
        ty.name(),
        kind.slug(),
        side_key(rust, false),
        side_key(go, true)
    )
}

fn side_key(s: &Side, go: bool) -> String {
    match s {
        Side::Accepted(_) => "ok".into(),
        Side::Rejected(m) if go => normalize(m),
        Side::Rejected(m) => m.clone(),
        Side::Panicked(m) if go => format!("panic {}", normalize(m)),
        Side::Panicked(_) => "panic".into(),
        Side::Failed(m) => format!("failure {}", normalize(m)),
    }
}

/// The innermost `decoding <Type>: ...` part of a go message, with quoted values and digit
/// runs blanked. go wraps an error once per enclosing type, so the same refusal reached
/// through two different paths would otherwise be two classes.
pub fn normalize(message: &str) -> String {
    let parts: Vec<&str> = message.split(": ").collect();
    let from = parts
        .iter()
        .rposition(|p| p.starts_with("decoding "))
        .unwrap_or(0);
    let joined = parts[from..].join(": ");
    let m = joined.as_str();
    let mut out = String::with_capacity(m.len());
    let mut chars = m.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            for d in chars.by_ref() {
                if d == '\'' {
                    break;
                }
            }
            out.push_str("'_'");
        } else if c.is_ascii_digit() {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
            out.push('N');
        } else {
            out.push(c);
        }
    }
    out.chars().take(240).collect()
}

/// rs-stellar-xdr in process, as core calls it: `T::from_xdr` then `to_xdr`, same limits.
pub fn rust_side(ty: FuzzType, bytes: &[u8], limits: Limits) -> Side {
    match catch_unwind(AssertUnwindSafe(|| ty.roundtrip(bytes, limits))) {
        Ok(Ok(b)) => Side::Accepted(b),
        Ok(Err(e)) => Side::Rejected(rust_error_kind(&e)),
        Err(p) => Side::Panicked(panic_text(p.as_ref())),
    }
}

/// The error variant without its payload, which is all a class should key on.
pub fn rust_error_kind(e: &stellar_xdr::Error) -> String {
    match e {
        stellar_xdr::Error::Io(io) => format!("Io({:?})", io.kind()),
        stellar_xdr::Error::Utf8Error(_) => "Utf8Error".into(),
        other => {
            let d = format!("{other:?}");
            d.split('(').next().unwrap_or(&d).to_string()
        }
    }
}

fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".into()
    }
}

/// go-stellar-sdk through the testee protocol. Errors only when the testee cannot be
/// restarted; a timeout or crash of one request is a [`Side::Failed`].
pub fn go_side(go: &mut Testee, ty: FuzzType, bytes: &[u8], depth: u32) -> Result<Side> {
    let request = Map::from_iter([
        ("op".to_string(), Value::from("decode_encode")),
        ("type".to_string(), Value::from(ty.name())),
        ("xdr".to_string(), Value::from(B64.encode(bytes))),
        ("depth_limit".to_string(), Value::from(depth)),
        ("len_limit".to_string(), Value::from(bytes.len())),
    ]);
    Ok(match go.request(request)? {
        Ok(Reply::Ok(v)) => match v.get("xdr").and_then(Value::as_str) {
            Some(text) => match B64.decode(text) {
                Ok(b) => Side::Accepted(b),
                Err(e) => Side::Failed(format!("result xdr is not base64: {e}")),
            },
            None => Side::Failed("ok result without xdr".into()),
        },
        Ok(Reply::Error { class, message }) => match class {
            ErrorClass::Rejected => Side::Rejected(message),
            ErrorClass::Panic => Side::Panicked(message),
            ErrorClass::BadRequest | ErrorClass::Internal => {
                Side::Failed(format!("{}: {message}", class.as_str()))
            }
        },
        Ok(Reply::Unsupported(reason)) => {
            bail!(
                "the go testee does not support decode_encode of {}: {reason}",
                ty.name()
            )
        }
        Err(failure) => Side::Failed(failure.to_string()),
    })
}

/// rust refuses on depth alone and go reads it.
pub const DEPTH_GO_DEEPER: &str = "depth-accounting-go-deeper";
/// go refuses on depth alone and rust reads it.
pub const DEPTH_GO_SHALLOWER: &str = "depth-accounting-go-shallower";

/// Why a disagreement is the known depth-accounting divergence and not a new class.
///
/// go-stellar-sdk's `MaxDepth` is spent per generated `DecodeFrom` call, rs-stellar-xdr's
/// per `read_xdr` of every type including the wrappers and boxes, so at the same number the
/// two refuse at different nesting (in go a bare ScVal holds 499 ScVec levels at 1000; see
/// conformance/known-failures/go.txt). A disagreement is that divergence,
/// and nothing else, when lifting the refusing side's depth limit, and only that, makes it
/// accept with exactly the bytes the other side produced.
pub fn known_filter(
    go: &mut Testee,
    ty: FuzzType,
    bytes: &[u8],
    rust: &Side,
    go_side_now: &Side,
) -> Result<Option<&'static str>> {
    match (rust, go_side_now) {
        (Side::Rejected(r), Side::Accepted(g)) if r == "DepthLimitExceeded" => {
            let unlimited = Limits {
                depth: u32::MAX,
                len: bytes.len(),
            };
            Ok((rust_side(ty, bytes, unlimited).accepted() == Some(g)).then_some(DEPTH_GO_DEEPER))
        }
        (Side::Accepted(r), Side::Rejected(m)) if m.contains("maximum decoding depth reached") => {
            let again = go_side(go, ty, bytes, u32::MAX)?;
            Ok((again.accepted() == Some(r)).then_some(DEPTH_GO_SHALLOWER))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_blanks_values() {
        assert_eq!(
            normalize(
                "xdr.UnmarshalWithOptions: decoding Bool: xdr:DecodeBool: bool not 0 or 1 - read: '2'"
            ),
            "decoding Bool: xdr:DecodeBool: bool not N or N - read: '_'"
        );
        assert_eq!(
            normalize(
                "xdr.UnmarshalWithOptions: decoding ScMap: decoding ScMapEntry: decoding ScVal: decoding Bool: x"
            ),
            "decoding Bool: x"
        );
    }

    #[test]
    fn classify_agreement() {
        let a = Side::Accepted(vec![0, 0, 0, 1]);
        let r = Side::Rejected("Invalid".into());
        assert_eq!(classify(&a, &a), None);
        assert_eq!(classify(&r, &Side::Rejected("x".into())), None);
        assert_eq!(classify(&a, &r), Some(Kind::RustAcceptsGoRejects));
        assert_eq!(classify(&r, &a), Some(Kind::GoAcceptsRustRejects));
        assert_eq!(
            classify(&a, &Side::Accepted(vec![0, 0, 0, 2])),
            Some(Kind::ReencodeDiffers)
        );
        assert_eq!(
            classify(&a, &Side::Failed("t".into())),
            Some(Kind::GoFailure)
        );
    }

    #[test]
    fn rust_side_matches_core_limits() {
        // ScVal u32 7, then a trailing byte that a whole-input decode refuses
        let ok = [0, 0, 0, 3, 0, 0, 0, 7];
        assert_eq!(
            rust_side(FuzzType::ScVal, &ok, FuzzType::ScVal.limits(ok.len())),
            Side::Accepted(ok.to_vec())
        );
        let trailing = [0, 0, 0, 3, 0, 0, 0, 7, 0];
        assert!(matches!(
            rust_side(
                FuzzType::ScVal,
                &trailing,
                FuzzType::ScVal.limits(trailing.len())
            ),
            Side::Rejected(_)
        ));
    }
}
