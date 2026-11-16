//! An audit report as Markdown, for a CI job summary.

use std::fmt::Write;

use shapekit_contracts::AuditReport;
use shapekit_contracts::audit::AuditNetwork;

/// Ascending ledgers as runs of consecutive ones, `first-last` or a single number.
pub fn runs(ledgers: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &l in ledgers {
        match out.last_mut() {
            Some((_, end)) if end.checked_add(1) == Some(l) => *end = l,
            _ => out.push((l, l)),
        }
    }
    out
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn markdown(r: &AuditReport) -> String {
    let mut s = String::new();
    let network = match r.network {
        AuditNetwork::Pubnet => "pubnet",
        AuditNetwork::Testnet => "testnet",
    };
    let kind = if r.seed.is_some() {
        "random audit"
    } else {
        "sentinel window"
    };
    let _ = writeln!(s, "## {network} {kind}\n");
    if let Some(seed) = r.seed {
        let _ = writeln!(s, "- seed: {seed}");
    }
    let n = r.ledgers.len() as u64;
    let _ = writeln!(s, "- ledgers: {}", thousands(n));
    let rs = runs(&r.ledgers);
    if r.seed.is_none() {
        let shown: Vec<String> = rs
            .iter()
            .map(|(a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}-{b}")
                }
            })
            .collect();
        let _ = writeln!(s, "- ranges: {}", shown.join(", "));
    }
    let _ = writeln!(
        s,
        "- protocols: {} ({})",
        r.protocols.len(),
        r.protocols
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(s, "- transactions: {}", thousands(r.transactions));
    if let Some(e) = &r.envelopes {
        let _ = writeln!(
            s,
            "- envelopes: {} v0, {} v1, {} fee bump",
            thousands(e.v0),
            thousands(e.v1),
            thousands(e.fee_bump)
        );
    }
    if let Some(sdk) = &r.sdk {
        let _ = writeln!(s, "- sdk: {} {}", sdk.sdk, sdk.version);
    }
    let total = r.total_violations();
    let _ = writeln!(s, "- violations: {}", thousands(total));
    if total == 0 && r.seed.is_some() && n > 0 {
        let _ = writeln!(
            s,
            "\nNo violation in {} ledgers drawn at random: the share of ledgers that violate a law is below 3/{} at 95% confidence.",
            thousands(n),
            thousands(n)
        );
    }
    let _ = writeln!(s, "\n| law | checked | violations |\n|---|---:|---:|");
    for l in &r.laws {
        let _ = writeln!(
            s,
            "| {} | {} | {} |",
            l.law,
            thousands(l.checked),
            thousands(l.violations)
        );
    }
    let examples: Vec<_> = r
        .laws
        .iter()
        .flat_map(|l| l.examples.iter().map(move |e| (l.law.as_str(), e)))
        .collect();
    if !examples.is_empty() {
        let _ = writeln!(s, "\n### Violations\n");
        for (law, e) in examples {
            let _ = writeln!(s, "- {law}, ledger {}: {}", e.ledger, e.detail);
        }
    }
    if let Some(c) = &r.coverage {
        let _ = writeln!(
            s,
            "\n### Coverage\n\n- baseline `{}`: {} ranges\n- with this run: {} ranges",
            c.baseline,
            thousands(c.baseline_ranges),
            thousands(c.ranges)
        );
        for b in &c.new {
            let _ = writeln!(
                s,
                "- new: `{}`, first reached by ledger {}",
                b.location, b.ledger
            );
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_and_thousands() {
        assert_eq!(runs(&[1, 2, 3, 7, 9, 10]), vec![(1, 3), (7, 7), (9, 10)]);
        assert_eq!(runs(&[]), vec![]);
        assert_eq!(thousands(505_048), "505,048");
        assert_eq!(thousands(12), "12");
        assert_eq!(thousands(1_000_000), "1,000,000");
    }
}
