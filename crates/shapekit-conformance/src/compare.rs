//! Compares a testee's answer with the expected one and names the result.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::plan::Expect;
use crate::testee::{ErrorClass, Failure, Reply};

/// The result of a check, an entry or a reproduction cell. Declared from best to worst so that
/// `max` gives an entry's result from its checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResultClass {
    Pass,
    /// A check that agrees with `expected_json` on the testee whose library rendered it: the
    /// rust testee at the generator's stellar-xdr version. Its agreement is that library
    /// agreeing with itself, so it is shown apart from passes and never counted as independent
    /// evidence. An entry whose other checks pass still passes.
    Reference,
    Unsupported,
    DivergesFromPeers,
    DivergesFromNetwork,
    Error,
}

impl ResultClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ResultClass::Pass => "pass",
            ResultClass::Reference => "reference",
            ResultClass::Unsupported => "unsupported",
            ResultClass::DivergesFromPeers => "diverges-from-peers",
            ResultClass::DivergesFromNetwork => "diverges-from-network",
            ResultClass::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<ResultClass> {
        Some(match s {
            "pass" => ResultClass::Pass,
            "reference" => ResultClass::Reference,
            "unsupported" => ResultClass::Unsupported,
            "diverges-from-peers" => ResultClass::DivergesFromPeers,
            "diverges-from-network" => ResultClass::DivergesFromNetwork,
            "error" => ResultClass::Error,
            _ => return None,
        })
    }

    /// A result that fails a run unless a known-failures line covers it.
    pub fn is_failure(self) -> bool {
        matches!(
            self,
            ResultClass::DivergesFromNetwork | ResultClass::DivergesFromPeers | ResultClass::Error
        )
    }
}

impl std::fmt::Display for ResultClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One check's verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub result: ResultClass,
    pub detail: Option<String>,
    /// A short form of what the testee answered, for the matrix and issue bodies.
    pub actual: Value,
    /// The sdk panicked or the testee process died on this input. A crash is a robustness
    /// defect whatever protocol the input is from, so it is never downgraded to unsupported.
    pub crashed: bool,
}

/// The comparable outcome of an implementation-defined check, decided across testees later.
#[derive(Debug, Clone, PartialEq)]
pub enum Judged {
    Done(Verdict),
    Peer(Value),
}

/// Strings longer than this are cut in reports; the corpus file holds the full value.
const SHOW: usize = 512;

pub fn short(v: &Value) -> Value {
    match v {
        Value::String(s) if s.len() > SHOW => {
            let mut cut = SHOW;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            Value::String(format!("{}... ({} chars)", &s[..cut], s.len()))
        }
        Value::Array(items) => Value::Array(items.iter().map(short).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), short(v))).collect()),
        other => other.clone(),
    }
}

fn verdict(result: ResultClass, detail: impl Into<Option<String>>, actual: Value) -> Judged {
    Judged::Done(Verdict {
        result,
        detail: detail.into(),
        actual: short(&actual),
        crashed: false,
    })
}

fn crash(result: ResultClass, detail: String, actual: Value) -> Judged {
    Judged::Done(Verdict {
        result,
        detail: Some(detail),
        actual: short(&actual),
        crashed: true,
    })
}

/// The expected answer in the same short form as `actual`.
pub fn expected_view(expect: &Expect) -> Value {
    short(&match expect {
        Expect::Bytes(b) => json!({"xdr": b}),
        Expect::Accepted => json!("decodes"),
        Expect::Json(j) => json!({"json": j}),
        Expect::AnyJson(j) => json!({"any json, reference": j}),
        Expect::Rejected => json!("rejected"),
        Expect::Order(o) => json!({"order": o}),
        Expect::PoolId(p) => json!({"pool_id": p}),
        Expect::Valid(v) => json!({"valid": v}),
        Expect::Id(i) => json!({"id": i}),
        Expect::IdOrRejected(i) => json!({"either": [{"id": i}, "rejected"]}),
        Expect::Strkey { kind, payload } => json!({"kind": kind, "payload": payload}),
        Expect::Text(t) => json!({"text": t}),
        Expect::NoEntryLost { entries } => {
            json!({"native": "a map keeping all entries", "entries": entries})
        }
        Expect::Peers => json!("the same answer as every other sdk"),
        Expect::Unmappable(why) => json!({"runner-cannot-map-case": why}),
    })
}

pub fn judge(expect: &Expect, answer: &Result<Reply, Failure>) -> Judged {
    if let Expect::Unmappable(why) = expect {
        return verdict(
            ResultClass::Error,
            format!("runner cannot map this corpus case: {why}"),
            Value::Null,
        );
    }
    let reply = match answer {
        Err(failure @ Failure::Exited { .. }) => {
            return crash(ResultClass::Error, failure.to_string(), Value::Null);
        }
        Err(failure) => return verdict(ResultClass::Error, failure.to_string(), Value::Null),
        Ok(r) => r,
    };
    match reply {
        Reply::Unsupported(reason) => {
            return verdict(
                ResultClass::Unsupported,
                reason.clone(),
                json!("unsupported"),
            );
        }
        Reply::Error { class, message } => match class {
            ErrorClass::BadRequest | ErrorClass::Internal => {
                return verdict(
                    ResultClass::Error,
                    format!("testee answered {}: {message}", class.as_str()),
                    json!({"error": class.as_str()}),
                );
            }
            ErrorClass::Panic => {
                return crash(
                    ResultClass::DivergesFromNetwork,
                    format!("the sdk crashed instead of answering: {message}"),
                    json!({"error": "panic", "message": message}),
                );
            }
            ErrorClass::Rejected => {}
        },
        Reply::Ok(_) => {}
    }
    let rejected_message = match reply {
        Reply::Error { message, .. } => Some(message.clone()),
        _ => None,
    };
    let result = match reply {
        Reply::Ok(r) => Some(r),
        _ => None,
    };

    if let Expect::Peers = expect {
        let outcome = match (&rejected_message, result) {
            (Some(_), _) => json!("rejected"),
            (None, Some(r)) => r.clone(),
            (None, None) => unreachable!("an ok or rejected reply"),
        };
        return Judged::Peer(outcome);
    }

    let Some(result) = result else {
        let message = rejected_message.unwrap_or_default();
        return match expect {
            Expect::Rejected | Expect::NoEntryLost { .. } | Expect::IdOrRejected(_) => {
                verdict(ResultClass::Pass, None, json!({"rejected": message}))
            }
            _ => verdict(
                ResultClass::DivergesFromNetwork,
                format!("the sdk refused a value the network accepts: {message}"),
                json!({"rejected": message}),
            ),
        };
    };

    let member = |name: &str| result.get(name);
    let missing = |name: &str| {
        verdict(
            ResultClass::Error,
            format!("protocol violation: result has no valid {name}"),
            result.clone(),
        )
    };
    let compare = |ok: bool, what: &str| {
        if ok {
            verdict(ResultClass::Pass, None, result.clone())
        } else {
            verdict(
                ResultClass::DivergesFromNetwork,
                what.to_string(),
                result.clone(),
            )
        }
    };

    match expect {
        Expect::Rejected => verdict(
            ResultClass::DivergesFromNetwork,
            "the sdk accepted a value the network refuses".to_string(),
            result.clone(),
        ),
        Expect::Accepted => verdict(ResultClass::Pass, None, result.clone()),
        Expect::Bytes(b) => match member("xdr").and_then(Value::as_str) {
            Some(x) => compare(x == b, "the sdk's bytes differ from the expected bytes"),
            None => missing("xdr"),
        },
        Expect::AnyJson(_) => match member("json") {
            Some(_) => verdict(ResultClass::Pass, None, result.clone()),
            None => missing("json"),
        },
        Expect::Json(j) => match member("json") {
            Some(x) => compare(x == j, "the sdk's XDR-JSON differs from the expected value"),
            None => missing("json"),
        },
        Expect::Order(o) => match member("order").and_then(Value::as_i64) {
            Some(x) if (-1..=1).contains(&x) => {
                compare(x == *o, "the sdk orders the pair differently from core")
            }
            _ => missing("order"),
        },
        Expect::PoolId(p) => match member("pool_id").and_then(Value::as_str) {
            Some(x) => compare(x == p, "the sdk's pool id differs from core's"),
            None => missing("pool_id"),
        },
        Expect::Valid(v) => match member("valid").and_then(Value::as_bool) {
            Some(x) => compare(
                x == *v,
                if *v {
                    "the sdk refuses a map the host accepts"
                } else {
                    "the sdk accepts a map the host refuses"
                },
            ),
            None => missing("valid"),
        },
        Expect::Id(i) => match member("id").and_then(Value::as_str) {
            Some(x) => compare(x == i, "the sdk reads a different id"),
            None => missing("id"),
        },
        Expect::IdOrRejected(i) => match member("id").and_then(Value::as_str) {
            Some(x) => compare(
                x == i,
                "the sdk reads a different id; it must read this one exactly or refuse",
            ),
            None => missing("id"),
        },
        Expect::Strkey { kind, payload } => {
            match (
                member("kind").and_then(Value::as_str),
                member("payload").and_then(Value::as_str),
            ) {
                (Some(k), Some(p)) => compare(
                    k == kind && p == payload,
                    "the sdk decodes a different kind or payload",
                ),
                _ => missing("kind and payload"),
            }
        }
        Expect::Text(t) => match member("text").and_then(Value::as_str) {
            Some(x) => compare(x == t, "the sdk encodes different text"),
            None => missing("text"),
        },
        Expect::NoEntryLost { entries } => match member("native") {
            Some(native) => match native_map_entries(native) {
                Some(n) => compare(
                    n == *entries,
                    &format!(
                        "the native projection keeps {n} of {entries} map entries; the rest were lost silently"
                    ),
                ),
                None => compare(
                    false,
                    "the native projection is not a map, so the entries cannot all be there",
                ),
            },
            None => missing("native"),
        },
        Expect::Peers | Expect::Unmappable(_) => unreachable!("handled above"),
    }
}

fn native_map_entries(native: &Value) -> Option<usize> {
    if native.get("type").and_then(Value::as_str) != Some("map") {
        return None;
    }
    native
        .get("entries")
        .and_then(Value::as_array)
        .map(Vec::len)
}

/// Decides implementation-defined checks across sdks. `voters` marks, per outcome, the testee
/// that votes for its sdk: one per sdk (its `current` version when the run holds it), so an sdk
/// run at many versions does not outvote one run at few. The most common answer among the
/// votes is the majority; every testee, voter or not, whose outcome differs from it diverges
/// from its peers, and with a tie nobody agrees. With fewer than two votes there is no other
/// sdk to compare with, which is reported as such and never as a pass.
pub fn judge_peers(outcomes: &[Option<Value>], voters: &[bool]) -> Vec<Option<Verdict>> {
    assert_eq!(outcomes.len(), voters.len(), "one voter flag per outcome");
    let votes: Vec<&Value> = outcomes
        .iter()
        .zip(voters)
        .filter(|(_, v)| **v)
        .filter_map(|(o, _)| o.as_ref())
        .collect();
    let mut counts: Vec<(&Value, usize)> = Vec::new();
    for v in &votes {
        match counts.iter_mut().find(|(k, _)| k == v) {
            Some((_, n)) => *n += 1,
            None => counts.push((v, 1)),
        }
    }
    let top = counts.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let tied = counts.iter().filter(|(_, n)| *n == top).count() > 1;
    let done = |result, detail: Option<&str>, v: &Value| Verdict {
        result,
        detail: detail.map(String::from),
        actual: short(v),
        crashed: false,
    };
    outcomes
        .iter()
        .map(|o| {
            o.as_ref().map(|v| {
                if votes.len() < 2 {
                    return done(
                        ResultClass::Unsupported,
                        Some("implementation-defined and no other sdk answered to compare with"),
                        v,
                    );
                }
                let agrees = !tied && counts.iter().any(|(k, n)| *k == v && *n == top);
                if agrees && counts.len() == 1 {
                    done(ResultClass::Pass, None, v)
                } else if agrees {
                    // the majority answer is not wrong, but the entry still has disagreement
                    done(
                        ResultClass::Pass,
                        Some("agrees with the most common answer of the sdks"),
                        v,
                    )
                } else {
                    done(
                        ResultClass::DivergesFromPeers,
                        Some("answers differently from the other sdks"),
                        v,
                    )
                }
            })
        })
        .collect()
}
