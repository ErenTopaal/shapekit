//! The encoding laws of the law library on stellar-xdr: a value the crate decodes under the
//! corpus limits must encode back to exactly the input bytes, and the input with a word more
//! or a byte less must not decode. The first input byte picks the type, in the order of
//! `shapekit_fuzz::types::FuzzType::ALL`; the rest is the xdr.
//!
//! A violation panics with `shapekit-class <class>`; a class listed in the file named by
//! `SHAPEKIT_FUZZ_SKIP` is not reported again, so one run can go on past a class it found.
//! Classes are `<type>:roundtrip:<in>-><out>`, where `<in>` and `<out>` describe the first
//! 4-byte word that differs between the input and its re-encoding (`0`, `1`, or `x` for any
//! other value), `<type>:noncanonical:longer` or `:shorter` for an input with a word more or a
//! byte less that still decodes, and `<type>:panic`. The go and javascript targets name
//! classes the same way.
#![no_main]

use std::cell::Cell;
use std::collections::BTreeSet;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use libfuzzer_sys::fuzz_target;
use shapekit_laws::{Law, Report, check_encoding};
use stellar_xdr as x;
use stellar_xdr::{Limits, ReadXdr, WriteXdr};

thread_local! {
    static CURRENT: Cell<&'static str> = const { Cell::new("") };
}

fn skip() -> &'static BTreeSet<String> {
    static SKIP: OnceLock<BTreeSet<String>> = OnceLock::new();
    SKIP.get_or_init(|| {
        // a panic inside the crate is a class too; the hook names it before libfuzzer aborts
        std::panic::set_hook(Box::new(|info| {
            let ours = info
                .payload()
                .downcast_ref::<String>()
                .is_some_and(|m| m.starts_with("shapekit-class "));
            // libfuzzer's callback cannot unwind, so every panic is followed by a second one
            // that aborts; only the first names a class
            static SEEN: AtomicBool = AtomicBool::new(false);
            if !ours && !SEEN.swap(true, Ordering::SeqCst) {
                let ty = CURRENT.with(Cell::get);
                eprintln!("shapekit-class {ty}:panic");
            }
            SEEN.store(true, Ordering::SeqCst);
            eprintln!("{info}");
        }));
        match std::env::var("SHAPEKIT_FUZZ_SKIP") {
            Ok(path) => std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("reading {path}: {e}"))
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
            Err(_) => BTreeSet::new(),
        }
    })
}

fn check<T: ReadXdr + WriteXdr>(name: &'static str, bytes: &[u8]) {
    CURRENT.with(|c| c.set(name));
    // the corpus limits first: without a depth limit a deep enough input exhausts the stack,
    // which is the decoder doing what it was told, not a law broken
    if T::from_xdr(bytes, shapekit_manifest::decode_limits(name, bytes.len())).is_err() {
        return;
    }
    let mut report = Report::new();
    let value = check_encoding::<T>(bytes, &mut report);
    for v in &report.violations {
        let class = match v.law {
            Law::Roundtrip => {
                let again = value
                    .as_ref()
                    .ok()
                    .and_then(|v| v.to_xdr(Limits::none()).ok())
                    .unwrap_or_default();
                format!("{name}:roundtrip:{}", first_word_change(bytes, &again))
            }
            Law::NoncanonicalRejected if v.detail.contains("trailing") => {
                format!("{name}:noncanonical:longer")
            }
            Law::NoncanonicalRejected => format!("{name}:noncanonical:shorter"),
            other => format!("{name}:{}", other.as_str()),
        };
        if !skip().contains(&class) {
            panic!("shapekit-class {class}: {}", v.detail);
        }
    }
}

fn word_kind(w: Option<&[u8]>) -> &'static str {
    match w {
        Some([0, 0, 0, 0]) => "0",
        Some([0, 0, 0, 1]) => "1",
        _ => "x",
    }
}

/// `<in>-><out>` of the first 4-byte word in which the two encodings differ.
fn first_word_change(input: &[u8], again: &[u8]) -> String {
    let words = input.len().max(again.len()).div_ceil(4);
    for i in 0..words {
        let a = input.get(i * 4..(i * 4 + 4).min(input.len()));
        let b = again.get(i * 4..(i * 4 + 4).min(again.len()));
        if a != b {
            return format!("{}->{}", word_kind(a), word_kind(b));
        }
    }
    "x->x".to_string()
}

macro_rules! dispatch {
    ($sel:expr, $bytes:expr, $($t:ident),* $(,)?) => {{
        const N: usize = [$(stringify!($t)),*].len();
        let mut i = 0usize;
        let pick = usize::from($sel) % N;
        $(
            if i == pick { check::<x::$t>(stringify!($t), $bytes); }
            i += 1;
        )*
        let _ = i;
    }};
}

fuzz_target!(|data: &[u8]| {
    skip();
    let Some((&sel, bytes)) = data.split_first() else {
        return;
    };
    // the same order as shapekit_fuzz::types::FuzzType::ALL
    dispatch!(
        sel,
        bytes,
        TransactionEnvelope,
        LedgerCloseMeta,
        TransactionResultMeta,
        TransactionResultMetaV1,
        LedgerEntry,
        ContractEvent,
        ScVal,
        SorobanAuthorizationEntry,
        ScSpecFunctionV0,
        Asset,
        SignerKey,
    );
});
