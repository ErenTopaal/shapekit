//! Cross-file consistency rules for the registry.

use std::collections::{BTreeSet, HashSet};

use crate::{
    CAPS_FILE, COVERED_THROUGH_PROTOCOL, Derivation, FAMILIES_FILE, FIELDS_FILE, LIMITS_FILE,
    LINT_RULE_IDS, LINT_RULES_FILE, Problem, ProtocolRange, Registry, SCHEMA_VERSION,
};

pub(crate) fn run(reg: &Registry) -> Vec<Problem> {
    let mut out = Vec::new();
    schema_versions(reg, &mut out);
    caps(reg, &mut out);
    fields(reg, &mut out);
    limits(reg, &mut out);
    lint_rules(reg, &mut out);
    families(reg, &mut out);
    out
}

fn schema_versions(reg: &Registry, out: &mut Vec<Problem>) {
    let mut versions = vec![
        (CAPS_FILE, reg.caps.schema_version),
        (FIELDS_FILE, reg.fields.schema_version),
        (LIMITS_FILE, reg.limits.schema_version),
        (LINT_RULES_FILE, reg.lint_rules.schema_version),
    ];
    if let Some(f) = &reg.families {
        versions.push((FAMILIES_FILE, f.schema_version));
    }
    for (file, v) in versions {
        if v != SCHEMA_VERSION {
            out.push(Problem::new(
                file,
                format!("schema_version is {v}, this build reads {SCHEMA_VERSION}"),
            ));
        }
    }
}

fn unique<'a>(file: &str, kind: &str, ids: impl Iterator<Item = &'a str>, out: &mut Vec<Problem>) {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() {
            out.push(Problem::new(file, format!("a {kind} has an empty id")));
        } else if !seen.insert(id) {
            out.push(Problem::new(
                format!("{file} {id}"),
                format!("{kind} id is used more than once"),
            ));
        }
    }
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

// CAP-0040, or CAP-0046-01 for one document of a multi-part CAP
fn valid_cap_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("CAP-") else {
        return false;
    };
    let (num, part) = match rest.split_once('-') {
        Some((n, p)) => (n, Some(p)),
        None => (rest, None),
    };
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    digits(num, 4) && part.is_none_or(|p| digits(p, 2))
}

fn caps(reg: &Registry, out: &mut Vec<Problem>) {
    unique(
        CAPS_FILE,
        "cap",
        reg.caps.caps.iter().map(|c| c.id.as_str()),
        out,
    );
    for cap in &reg.caps.caps {
        let at = format!("{CAPS_FILE} {}", cap.id);
        if !valid_cap_id(&cap.id) {
            out.push(Problem::new(
                &at,
                "id must look like CAP-0040 or CAP-0046-01",
            ));
        }
        if blank(&cap.title) || blank(&cap.status) {
            out.push(Problem::new(&at, "title and status are required"));
        }
        if blank(&cap.source.path) {
            out.push(Problem::new(&at, "source.path is empty"));
        }
        if cap.protocol == Some(0) {
            out.push(Problem::new(&at, "protocol 0 is not a real protocol"));
        }
        if !cap.indexer_visible && cap.reason.as_deref().is_none_or(blank) {
            out.push(Problem::new(
                &at,
                "indexer_visible is false but no reason is given",
            ));
        }
        if cap.indexer_visible && cap.xdr_additions.iter().any(|x| blank(x)) {
            out.push(Problem::new(&at, "xdr_additions has an empty entry"));
        }
    }
}

// protocol 0 is genesis, so a field checked since the first ledger starts at 0
fn range_ok(min: u32, max: Option<u32>) -> bool {
    max.is_none_or(|m| m >= min)
}

fn fields(reg: &Registry, out: &mut Vec<Problem>) {
    unique(
        FIELDS_FILE,
        "field",
        reg.fields.fields.iter().map(|f| f.id.as_str()),
        out,
    );
    for field in &reg.fields.fields {
        let at = format!("{FIELDS_FILE} {}", field.id);
        if blank(&field.xdr_path) {
            out.push(Problem::new(&at, "xdr_path is empty"));
        }
        if !range_ok(field.protocol_min, field.protocol_max) {
            out.push(Problem::new(
                &at,
                format!(
                    "protocol range {}..{:?} is not sane",
                    field.protocol_min, field.protocol_max
                ),
            ));
        }
        if field.citations.is_empty() {
            out.push(Problem::new(&at, "has no source citation"));
        }
        for (i, c) in field.citations.iter().enumerate() {
            let missing: Vec<&str> = [
                ("repo", &c.repo),
                ("ref", &c.git_ref),
                ("path", &c.path),
                ("symbol", &c.symbol),
            ]
            .into_iter()
            .filter(|(_, v)| blank(v))
            .map(|(k, _)| k)
            .collect();
            if !missing.is_empty() {
                out.push(Problem::new(
                    &at,
                    format!("citation {i} has empty {}", missing.join(", ")),
                ));
            }
        }
    }
}

fn limits(reg: &Registry, out: &mut Vec<Problem>) {
    unique(
        LIMITS_FILE,
        "limit",
        reg.limits.limits.iter().map(|l| l.id.as_str()),
        out,
    );
    for limit in &reg.limits.limits {
        let at = format!("{LIMITS_FILE} {}", limit.id);
        if limit.recommended <= 0 {
            out.push(Problem::new(&at, "recommended must be positive"));
        }
        if limit.derived_from.is_empty() {
            out.push(Problem::new(&at, "derived_from names no upstream constant"));
        }
        if limit.protocol_min == Some(0) {
            out.push(Problem::new(&at, "protocol_min 0 is not a real protocol"));
        }
        for (i, d) in limit.derived_from.iter().enumerate() {
            let required: Vec<(&str, &str)> = match d {
                Derivation::SourceCode(c) => vec![
                    ("repo", &c.repo),
                    ("ref", &c.git_ref),
                    ("path", &c.path),
                    ("symbol", &c.symbol),
                ],
                Derivation::Measurement(m) => vec![
                    ("repo", &m.repo),
                    ("ref", &m.git_ref),
                    ("path", &m.path),
                    ("symbol", &m.symbol),
                    ("measured_with", &m.measured_with),
                ],
                Derivation::NetworkSetting(n) => vec![
                    ("network", &n.network),
                    ("rpc", &n.rpc),
                    ("setting", &n.setting),
                    ("read_at", &n.read_at),
                ],
            };
            let missing: Vec<&str> = required
                .into_iter()
                .filter(|(_, v)| blank(v))
                .map(|(k, _)| k)
                .collect();
            if !missing.is_empty() {
                out.push(Problem::new(
                    &at,
                    format!("derived_from {i} has empty {}", missing.join(", ")),
                ));
            }
        }
        if limit.network_setting
            && !limit
                .derived_from
                .iter()
                .any(|d| matches!(d, Derivation::NetworkSetting(_)))
        {
            out.push(Problem::new(
                &at,
                "network_setting is true but no network setting is cited",
            ));
        }
    }
}

fn lint_rules(reg: &Registry, out: &mut Vec<Problem>) {
    unique(
        LINT_RULES_FILE,
        "rule",
        reg.lint_rules.rules.iter().map(|r| r.id.as_str()),
        out,
    );
    let have: BTreeSet<&str> = reg.lint_rules.rules.iter().map(|r| r.id.as_str()).collect();
    let want: BTreeSet<&str> = LINT_RULE_IDS.into_iter().collect();
    for id in want.difference(&have) {
        out.push(Problem::new(
            LINT_RULES_FILE,
            format!("rule {id} is missing"),
        ));
    }
    for id in have.difference(&want) {
        out.push(Problem::new(
            LINT_RULES_FILE,
            format!("rule {id} is not one of {}", LINT_RULE_IDS.join(", ")),
        ));
    }
    for rule in &reg.lint_rules.rules {
        if [
            &rule.name,
            &rule.description,
            &rule.rationale,
            &rule.checklist,
        ]
        .into_iter()
        .any(|s| blank(s))
        {
            out.push(Problem::new(
                format!("{LINT_RULES_FILE} {}", rule.id),
                "name, description, rationale and checklist are all required",
            ));
        }
    }
}

// family ids are dotted, lowercase: cap40.signed-payload-signer, classic.max-length
fn valid_family_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn families(reg: &Registry, out: &mut Vec<Problem>) {
    let Some(file) = &reg.families else {
        out.push(Problem::new(FAMILIES_FILE, "families.toml missing"));
        return;
    };
    unique(
        FAMILIES_FILE,
        "family",
        file.families.iter().map(|f| f.id.as_str()),
        out,
    );
    for fam in &file.families {
        let at = format!("{FAMILIES_FILE} {}", fam.id);
        if !valid_family_id(&fam.id) {
            out.push(Problem::new(
                &at,
                "id must be dotted lowercase, like cap40.signed-payload-signer",
            ));
        }
        if blank(&fam.title) {
            out.push(Problem::new(&at, "title is empty"));
        }
        if fam.caps.is_empty() {
            out.push(Problem::new(&at, "names no cap"));
        }
        for cap_id in &fam.caps {
            match reg.cap(cap_id) {
                None => out.push(Problem::new(&at, format!("unknown cap {cap_id}"))),
                Some(cap) => {
                    if let Some(p) = cap.protocol
                        && fam.protocol_introduced < p
                    {
                        out.push(Problem::new(
                            &at,
                            format!(
                                "protocol_introduced {} is before {cap_id}'s protocol {p}",
                                fam.protocol_introduced
                            ),
                        ));
                    }
                }
            }
        }
        for field_id in &fam.fields {
            if reg.field(field_id).is_none() {
                out.push(Problem::new(&at, format!("unknown field {field_id}")));
            }
        }
        let ProtocolRange { min, max } = fam.protocol_valid;
        if !range_ok(min, max) {
            out.push(Problem::new(
                &at,
                format!("protocol_valid {min}..{max:?} is not sane"),
            ));
        } else if fam.protocol_introduced > min {
            out.push(Problem::new(
                &at,
                format!(
                    "protocol_valid.min {min} is before protocol_introduced {}",
                    fam.protocol_introduced
                ),
            ));
        }
        if fam.signature.is_empty() || fam.signature.iter().any(|s| blank(s)) {
            out.push(Problem::new(
                &at,
                "signature needs at least one non-empty fragment",
            ));
        }
        if fam.source_classes.is_empty() {
            out.push(Problem::new(&at, "source_classes is empty"));
        }
        let mut seen = HashSet::new();
        for class in &fam.source_classes {
            if !seen.insert(*class) {
                out.push(Problem::new(
                    &at,
                    format!("source class {} is listed twice", class.as_str()),
                ));
            }
        }
        for url in &fam.linked_issues {
            if !url.starts_with("https://") {
                out.push(Problem::new(
                    &at,
                    format!("linked issue {url} is not a URL"),
                ));
            }
        }
        if fam.priority == 0 {
            out.push(Problem::new(&at, "priority starts at 1"));
        }
    }

    let referenced: HashSet<&str> = file
        .families
        .iter()
        .flat_map(|f| f.caps.iter().map(String::as_str))
        .collect();
    for cap in &reg.caps.caps {
        let in_scope = cap.protocol.is_some_and(|p| p <= COVERED_THROUGH_PROTOCOL);
        if cap.indexer_visible && in_scope && !referenced.contains(cap.id.as_str()) {
            out.push(Problem::new(
                format!("{CAPS_FILE} {}", cap.id),
                "indexer-visible cap is not referenced by any family",
            ));
        }
    }
}
