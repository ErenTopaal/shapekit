//! Schema mutation: copies of the schema with one change each, and whether the format vectors
//! tell each copy from the real schema. A vector kills a mutant when the reference interpreter
//! over the mutant gives it a different verdict or a different value than over the real schema.
//! A mutant no vector kills shows a decision the vectors do not pin down.

use serde::Serialize;

use crate::interp::{Interp, Value};
use crate::names::rust_type_name;
use crate::vectors::Vector;
use crate::{Arm, Case, Definition, Member, Schema, Type};

/// One single-change copy of the schema.
pub struct Mutant {
    /// What changed, for example `struct SCPBallot: swap fields counter and value`.
    pub description: String,
    /// The operator: `swap-fields`, `bound-up`, `bound-down`, `size-up`, `size-down`,
    /// `drop-arm`, `extra-arm`, `drop-member`, `extra-member`, `make-optional`,
    /// `make-required`.
    pub operator: &'static str,
    /// The definition the change is in.
    pub definition: String,
    pub schema: Schema,
}

fn bump(n: u64, up: bool) -> Option<u64> {
    if up {
        n.checked_add(1)
    } else {
        n.checked_sub(1)
    }
}

/// The type with its own bound or size moved by one, when it has one.
fn shift(ty: &Type, up: bool) -> Option<(Type, &'static str)> {
    Some(match ty {
        Type::OpaqueFixed { size } => (
            Type::OpaqueFixed {
                size: bump(*size, up).filter(|s| *s > 0)?,
            },
            if up { "size-up" } else { "size-down" },
        ),
        Type::Array { element, count } => (
            Type::Array {
                element: element.clone(),
                count: bump(*count, up).filter(|c| *c > 0)?,
            },
            if up { "size-up" } else { "size-down" },
        ),
        Type::OpaqueVar { max_size: Some(m) } => (
            Type::OpaqueVar {
                max_size: Some(bump(*m, up)?),
            },
            if up { "bound-up" } else { "bound-down" },
        ),
        Type::String { max_size: Some(m) } => (
            Type::String {
                max_size: Some(bump(*m, up)?),
            },
            if up { "bound-up" } else { "bound-down" },
        ),
        Type::VarArray {
            element,
            max_count: Some(m),
        } => (
            Type::VarArray {
                element: element.clone(),
                max_count: Some(bump(*m, up)?),
            },
            if up { "bound-up" } else { "bound-down" },
        ),
        _ => return None,
    })
}

/// Every single-change mutant of the named definitions.
pub fn mutants(schema: &Schema, names: &[&str]) -> Vec<Mutant> {
    let mut out = Vec::new();
    let mut add = |op: &'static str, name: &str, desc: String, d: Definition| {
        out.push(Mutant {
            description: desc,
            operator: op,
            definition: name.to_string(),
            schema: schema.with_definition(d),
        });
    };
    for name in names {
        let Some(def) = schema.get(name) else {
            continue;
        };
        match def {
            Definition::Struct(s) => {
                for i in 0..s.fields.len().saturating_sub(1) {
                    let mut m = s.clone();
                    m.fields.swap(i, i + 1);
                    add(
                        "swap-fields",
                        name,
                        format!(
                            "struct {name}: swap fields {} and {}",
                            s.fields[i].name,
                            s.fields[i + 1].name
                        ),
                        Definition::Struct(m),
                    );
                }
                for (i, f) in s.fields.iter().enumerate() {
                    for up in [true, false] {
                        if let Some((t, op)) = shift(&f.ty, up) {
                            let mut m = s.clone();
                            m.fields[i].ty = t;
                            add(
                                op,
                                name,
                                format!("struct {name}: field {} {op}", f.name),
                                Definition::Struct(m),
                            );
                        }
                    }
                    let mut m = s.clone();
                    let (t, op) = match &f.ty {
                        Type::Optional { element } => ((**element).clone(), "make-required"),
                        other => (
                            Type::Optional {
                                element: Box::new(other.clone()),
                            },
                            "make-optional",
                        ),
                    };
                    m.fields[i].ty = t;
                    add(
                        op,
                        name,
                        format!("struct {name}: field {} {op}", f.name),
                        Definition::Struct(m),
                    );
                }
            }
            Definition::Typedef(t) => {
                for up in [true, false] {
                    if let Some((ty, op)) = shift(&t.ty, up) {
                        let mut m = t.clone();
                        m.ty = ty;
                        add(
                            op,
                            name,
                            format!("typedef {name}: {op}"),
                            Definition::Typedef(m),
                        );
                    }
                }
            }
            Definition::Union(u) => {
                if u.arms.len() > 1 {
                    for (i, a) in u.arms.iter().enumerate() {
                        let mut m = u.clone();
                        m.arms.remove(i);
                        let v: Vec<String> = a.cases.iter().map(|c| c.value.to_string()).collect();
                        add(
                            "drop-arm",
                            name,
                            format!("union {name}: drop the arm for {}", v.join(", ")),
                            Definition::Union(m),
                        );
                    }
                }
                // a void arm for the smallest word no arm takes, the next value a schema change
                // would add
                let taken: Vec<i64> = u.case_values().collect();
                let members: Vec<(i64, String)> = match &u.discriminant.ty {
                    Type::Ref { name: d } => match schema.get(d) {
                        Some(Definition::Enum(e)) => e
                            .members
                            .iter()
                            .map(|m| (m.value, m.name.clone()))
                            .collect(),
                        _ => Vec::new(),
                    },
                    _ => Vec::new(),
                };
                let extra = members
                    .iter()
                    .find(|(v, _)| !taken.contains(v))
                    .map(|(v, n)| (*v, Some(n.clone())))
                    .or_else(|| {
                        members.is_empty().then(|| {
                            (
                                (0..).find(|w| !taken.contains(w)).expect("a free word"),
                                None,
                            )
                        })
                    });
                if let Some((value, case_name)) = extra {
                    let mut m = u.clone();
                    m.arms.push(Arm {
                        cases: vec![Case {
                            value,
                            name: case_name,
                        }],
                        name: None,
                        ty: None,
                    });
                    add(
                        "extra-arm",
                        name,
                        format!("union {name}: a void arm for {value}"),
                        Definition::Union(m),
                    );
                }
                for (i, a) in u.arms.iter().enumerate() {
                    if let Some(ty) = &a.ty {
                        for up in [true, false] {
                            if let Some((t, op)) = shift(ty, up) {
                                let mut m = u.clone();
                                m.arms[i].ty = Some(t);
                                add(
                                    op,
                                    name,
                                    format!("union {name}: arm {} {op}", a.cases[0].value),
                                    Definition::Union(m),
                                );
                            }
                        }
                    }
                }
            }
            Definition::Enum(e) => {
                if e.members.len() > 1 {
                    for (i, mbr) in e.members.iter().enumerate() {
                        let mut m = e.clone();
                        m.members.remove(i);
                        add(
                            "drop-member",
                            name,
                            format!("enum {name}: drop {}", mbr.name),
                            Definition::Enum(m),
                        );
                    }
                }
                let taken: Vec<i64> = e.members.iter().map(|m| m.value).collect();
                let value = (0..).find(|w| !taken.contains(w)).expect("a free word");
                let mut m = e.clone();
                m.members.push(Member {
                    name: format!("{}MUTANT_EXTRA", e.member_prefix),
                    value,
                });
                add(
                    "extra-member",
                    name,
                    format!("enum {name}: an extra member {value}"),
                    Definition::Enum(m),
                );
            }
            Definition::Const(_) => {}
        }
    }
    out
}

/// Whether one vector tells the mutant from the real schema.
pub fn kills(real: &Schema, mutant: &Schema, xdr_name: &str, v: &Vector) -> bool {
    let a = Interp::new(real).decode(xdr_name, &v.bytes);
    let b = Interp::new(mutant).decode(xdr_name, &v.bytes);
    match (a, b) {
        (Ok(x), Ok(y)) => {
            let root = Type::Ref {
                name: xdr_name.to_string(),
            };
            labelled(real, &root, &x) != labelled(mutant, &root, &y)
        }
        (Err(_), Err(_)) => false,
        _ => true,
    }
}

/// A value with its struct fields keyed by name, so two decoders that put the same numbers in
/// differently named fields give different trees.
#[derive(Debug, PartialEq, Eq)]
enum Labelled {
    Leaf(Value),
    List(Vec<Labelled>),
    Fields(std::collections::BTreeMap<String, Labelled>),
    Arm(i64, Option<Box<Labelled>>),
    Absent,
}

fn labelled(schema: &Schema, ty: &Type, v: &Value) -> Labelled {
    match (schema.resolve(ty), v) {
        (Type::Ref { name }, Value::Struct(fields)) => match schema.get(name) {
            Some(Definition::Struct(s)) => Labelled::Fields(
                s.fields
                    .iter()
                    .zip(fields)
                    .map(|(f, v)| (f.name.clone(), labelled(schema, &f.ty, v)))
                    .collect(),
            ),
            _ => Labelled::Leaf(v.clone()),
        },
        (Type::Ref { name }, Value::Union { discriminant, arm }) => match schema.get(name) {
            Some(Definition::Union(u)) => {
                let ty = u.arm(*discriminant).and_then(|a| a.ty.as_ref());
                Labelled::Arm(
                    *discriminant,
                    match (ty, arm) {
                        (Some(t), Some(a)) => Some(Box::new(labelled(schema, t, a))),
                        _ => None,
                    },
                )
            }
            _ => Labelled::Leaf(v.clone()),
        },
        (Type::Array { element, .. } | Type::VarArray { element, .. }, Value::Array(items)) => {
            Labelled::List(items.iter().map(|i| labelled(schema, element, i)).collect())
        }
        (Type::Optional { element }, Value::Optional(o)) => match o {
            None => Labelled::Absent,
            Some(inner) => labelled(schema, element, inner),
        },
        _ => Labelled::Leaf(v.clone()),
    }
}

/// The outcome for one mutant.
#[derive(Clone, Debug, Serialize)]
pub struct Outcome {
    pub description: String,
    pub operator: &'static str,
    pub definition: String,
    /// The obligation of the first vector that killed it.
    pub killed_by: Option<String>,
}

/// Runs every vector of every definition whose value can contain the mutated definition
/// against each mutant. `vectors` is keyed by the definition's XDR name.
pub fn check(
    real: &Schema,
    mutants: &[Mutant],
    vectors: &std::collections::BTreeMap<String, Vec<Vector>>,
) -> Vec<Outcome> {
    // which vector sets can hold each definition: its own, then every type reaching it
    let graph = real.graph();
    let mut holders: std::collections::BTreeMap<&str, Vec<&String>> = Default::default();
    for name in vectors.keys() {
        if let Ok(reach) = graph.reachable(name) {
            for d in reach {
                holders.entry(d).or_default().push(name);
            }
        }
    }
    mutants
        .iter()
        .map(|m| {
            let mut order: Vec<&String> = Vec::new();
            if let Some(own) = vectors.keys().find(|k| **k == m.definition) {
                order.push(own);
            }
            for h in holders.get(m.definition.as_str()).into_iter().flatten() {
                if **h != m.definition {
                    order.push(h);
                }
            }
            let killed_by = order.iter().find_map(|name| {
                vectors[*name]
                    .iter()
                    .find(|v| kills(real, &m.schema, name, v))
                    .map(|v| v.obligation.clone())
            });
            Outcome {
                description: m.description.clone(),
                operator: m.operator,
                definition: rust_type_name(&m.definition),
                killed_by,
            }
        })
        .collect()
}
