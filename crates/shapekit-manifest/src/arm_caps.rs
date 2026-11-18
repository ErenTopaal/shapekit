//! The CAP that introduced each union arm, read from `xdr_additions` in `registry/caps.toml`
//! and the pinned XDR definitions. An entry that records the arms it witnesses (`arms`) is
//! tagged with the CAPs of those arms, or with [`PRE_CAP`] when every one of them predates
//! every CAP.
//!
//! An arm `Type:case` of union `U` is matched against the names a CAP's `xdr_additions` use,
//! most specific first, and the first level with a match decides:
//! 1. the arm itself: its case constant (`BUMP_SEQUENCE`), `U.<arm>` (`TransactionEnvelope.v1`)
//!    or `S.<field>.<arm>` for a struct field of type `U` (`AccountEntry.ext.v1`);
//! 2. the arm's value type, a new type being only reachable through a new arm
//!    (`BumpSequenceOp`);
//! 3. the union itself: the CAP that introduced `U` brought every arm `U` started with.
//!
//! A definition is introduced by the CAPs that list it, its name or a struct field holding it
//! (`Transaction.cond`); a definition no CAP lists is introduced by the CAPs of every place
//! that refers to it, when each of those places came with a CAP (`ContractEventBody` is only
//! reachable through `ContractEvent`). An arm no level matches was in the XDR before the CAP
//! process.

use std::collections::{BTreeMap, BTreeSet};

use shapekit_registry::Registry;
use shapekit_schema::names::{case_name, rust_type_name};
use shapekit_schema::{Arm, Case, Definition, Schema, Type, Union};

/// The tag of a shape that predates every CAP. It is a record of `registry/caps.toml`, but
/// not a CAP.
pub use shapekit_registry::PRE_CAP;

type Caps = BTreeSet<String>;

/// The CAPs of every union arm of the pinned definitions, spelled as signatures spell arms.
pub struct ArmCaps {
    caps: BTreeMap<String, Caps>,
}

fn arm_name(arm: &Arm, value: i64) -> String {
    arm.name.clone().unwrap_or_else(|| format!("v{value}"))
}

/// Where a definition is referred from: a struct field or typedef of `from`, or an arm of the
/// union `from`.
enum Place<'a> {
    Plain(&'a str),
    Arm(&'a Union, &'a Arm, &'a Case),
}

struct Tables<'a> {
    listed: BTreeMap<&'a str, Caps>,
    /// The struct fields that hold each union, as `S.field`.
    holders: BTreeMap<&'a str, Vec<String>>,
}

impl Tables<'_> {
    fn lookup(&self, names: &[String]) -> Caps {
        names
            .iter()
            .filter_map(|n| self.listed.get(n.as_str()))
            .flatten()
            .cloned()
            .collect()
    }

    /// The CAPs of one arm, given what introduced each definition so far.
    fn arm(&self, u: &Union, arm: &Arm, case: &Case, intro: &BTreeMap<&str, Caps>) -> Caps {
        let held: &[String] = self.holders.get(u.name.as_str()).map_or(&[], Vec::as_slice);
        let name = arm_name(arm, case.value);
        let mut own: Vec<String> = case.name.iter().cloned().collect();
        own.push(format!("{}.{name}", u.name));
        own.extend(held.iter().map(|h| format!("{h}.{name}")));
        let found = self.lookup(&own);
        if !found.is_empty() {
            return found;
        }
        if let Some(Type::Ref { name }) = &arm.ty
            && name != &u.name
            && let Some(c) = self.listed.get(name.as_str())
        {
            return c.clone();
        }
        intro.get(u.name.as_str()).cloned().unwrap_or_default()
    }
}

impl ArmCaps {
    pub fn new(registry: &Registry) -> ArmCaps {
        Self::from_schema(&Schema::pinned(), registry)
    }

    pub fn from_schema(schema: &Schema, registry: &Registry) -> ArmCaps {
        let mut listed: BTreeMap<&str, Caps> = BTreeMap::new();
        for cap in &registry.caps.caps {
            for name in &cap.xdr_additions {
                listed
                    .entry(name.as_str())
                    .or_default()
                    .insert(cap.id.clone());
            }
        }
        let mut holders: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        // every place each definition is referred from
        let mut places: BTreeMap<&str, Vec<Place<'_>>> = BTreeMap::new();
        for d in schema.definitions() {
            match d {
                Definition::Struct(s) => {
                    for f in &s.fields {
                        if let Type::Ref { name } = &f.ty {
                            holders
                                .entry(name.as_str())
                                .or_default()
                                .push(format!("{}.{}", s.name, f.name));
                        }
                        f.ty.for_each_ref(&mut |r| {
                            places.entry(r).or_default().push(Place::Plain(&s.name));
                        });
                    }
                }
                Definition::Typedef(t) => t.ty.for_each_ref(&mut |r| {
                    places.entry(r).or_default().push(Place::Plain(&t.name));
                }),
                Definition::Union(u) => {
                    u.discriminant.ty.for_each_ref(&mut |r| {
                        places.entry(r).or_default().push(Place::Plain(&u.name));
                    });
                    for arm in &u.arms {
                        if let (Some(ty), Some(case)) = (&arm.ty, arm.cases.first()) {
                            ty.for_each_ref(&mut |r| {
                                places.entry(r).or_default().push(Place::Arm(u, arm, case));
                            });
                        }
                    }
                }
                Definition::Enum(_) | Definition::Const(_) => {}
            }
        }
        let tables = Tables { listed, holders };
        // what introduced each definition: its own listing, or a struct field holding it
        let mut intro: BTreeMap<&str, Caps> = BTreeMap::new();
        for d in schema.definitions() {
            let mut names = vec![d.name().to_owned()];
            names.extend(tables.holders.get(d.name()).into_iter().flatten().cloned());
            let found = tables.lookup(&names);
            if !found.is_empty() {
                intro.insert(d.name(), found);
            }
        }
        // then, until nothing changes, a definition every place of which came with a CAP; the
        // sets only grow, so this ends
        loop {
            let mut changed = false;
            for d in schema.definitions() {
                let name = d.name();
                if intro.contains_key(name) {
                    continue;
                }
                let Some(from) = places.get(name) else {
                    continue;
                };
                let mut caps = Caps::new();
                let mut all = true;
                for p in from {
                    let c = match p {
                        // a definition referring to itself says nothing about where it came from
                        Place::Plain(r) if *r == name => continue,
                        Place::Arm(u, ..) if u.name == name => continue,
                        Place::Plain(r) => intro.get(r).cloned().unwrap_or_default(),
                        Place::Arm(u, arm, case) => tables.arm(u, arm, case, &intro),
                    };
                    if c.is_empty() {
                        all = false;
                        break;
                    }
                    caps.extend(c);
                }
                if all && !caps.is_empty() {
                    intro.insert(name, caps);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut caps = BTreeMap::new();
        for d in schema.definitions() {
            let Definition::Union(u) = d else {
                continue;
            };
            for arm in &u.arms {
                for case in &arm.cases {
                    let key = format!("{}:{}", rust_type_name(&u.name), case_name(schema, u, case));
                    caps.insert(key, tables.arm(u, arm, case, &intro));
                }
            }
        }
        ArmCaps { caps }
    }

    /// The CAPs that introduced `arm`; empty for an arm older than every CAP, `None` for a name
    /// that is no union arm of the pinned definitions.
    pub fn of(&self, arm: &str) -> Option<&Caps> {
        self.caps.get(arm)
    }

    /// The caps an entry witnessing `arms` carries: the CAPs of its arms, sorted, or
    /// [`PRE_CAP`] alone when none of them came with a CAP. Errors name arms that are not
    /// union arms of the pinned definitions.
    pub fn for_arms<'a>(
        &self,
        arms: impl IntoIterator<Item = &'a String>,
    ) -> Result<Vec<String>, Vec<String>> {
        let mut out = Caps::new();
        let mut unknown = Vec::new();
        for a in arms {
            match self.of(a) {
                Some(c) => out.extend(c.iter().cloned()),
                None => unknown.push(a.clone()),
            }
        }
        if !unknown.is_empty() {
            return Err(unknown);
        }
        if out.is_empty() {
            out.insert(PRE_CAP.to_owned());
        }
        Ok(out.into_iter().collect())
    }
}
