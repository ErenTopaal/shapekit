//! The reference graph between definitions: what a root type reaches and which definitions
//! are recursive.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{Definition, Error, Kind, Schema};

/// Edges from each definition to the definitions its components name.
pub struct Graph<'a> {
    schema: &'a Schema,
    edges: BTreeMap<&'a str, BTreeSet<&'a str>>,
}

/// Counts over a set of definitions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    /// Definitions in the set.
    pub types: usize,
    /// Definitions by kind, every kind listed.
    pub kinds: BTreeMap<&'static str, usize>,
    /// Discriminant values with an arm, summed over the unions in the set.
    pub union_cases: usize,
    /// Members, summed over the enums in the set.
    pub enum_values: usize,
    /// Fields, summed over the structs in the set.
    pub struct_fields: usize,
}

impl<'a> Graph<'a> {
    pub fn new(schema: &'a Schema) -> Graph<'a> {
        let mut edges = BTreeMap::new();
        for d in schema.definitions() {
            let mut out = BTreeSet::new();
            for t in d.component_types() {
                t.for_each_ref(&mut |n| {
                    out.insert(n);
                });
            }
            edges.insert(d.name(), out);
        }
        Graph { schema, edges }
    }

    /// The definitions `name` refers to directly.
    pub fn children(&self, name: &str) -> impl Iterator<Item = &'a str> + '_ {
        self.edges.get(name).into_iter().flatten().copied()
    }

    /// Every definition reachable from `root`, the root included.
    pub fn reachable(&self, root: &str) -> Result<BTreeSet<&'a str>, Error> {
        let start = self.schema.require(root)?.name();
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(n) = stack.pop() {
            if seen.insert(n) {
                stack.extend(self.children(n));
            }
        }
        Ok(seen)
    }

    /// Counts over the definitions reachable from `root`, or over every definition.
    pub fn stats(&self, root: Option<&str>) -> Result<Stats, Error> {
        let names: Vec<&str> = match root {
            Some(r) => self.reachable(r)?.into_iter().collect(),
            None => self.schema.definitions().iter().map(|d| d.name()).collect(),
        };
        let mut s = Stats {
            kinds: Kind::ALL.iter().map(|k| (k.as_str(), 0)).collect(),
            ..Stats::default()
        };
        for n in names {
            let d = self.schema.require(n)?;
            s.types += 1;
            *s.kinds.entry(d.kind().as_str()).or_default() += 1;
            match d {
                Definition::Union(u) => s.union_cases += u.case_values().count(),
                Definition::Enum(e) => s.enum_values += e.members.len(),
                Definition::Struct(st) => s.struct_fields += st.fields.len(),
                Definition::Typedef(_) | Definition::Const(_) => {}
            }
        }
        Ok(s)
    }

    /// The strongly connected components that hold a cycle: groups of definitions that can
    /// contain themselves. Each group is sorted, the groups by their first name.
    pub fn recursive_groups(&self) -> Vec<Vec<&'a str>> {
        let mut groups: Vec<Vec<&str>> = tarjan(&self.edges)
            .into_iter()
            .filter(|c| c.len() > 1 || self.edges[c[0]].contains(c[0]))
            .map(|mut c| {
                c.sort_unstable();
                c
            })
            .collect();
        groups.sort();
        groups
    }
}

/// Tarjan's algorithm without recursion, so deep schemas cannot overflow the stack.
fn tarjan<'a>(edges: &BTreeMap<&'a str, BTreeSet<&'a str>>) -> Vec<Vec<&'a str>> {
    let mut index: BTreeMap<&str, usize> = BTreeMap::new();
    let mut low: BTreeMap<&str, usize> = BTreeMap::new();
    let mut on_stack: BTreeSet<&str> = BTreeSet::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    let mut next = 0;
    for &start in edges.keys() {
        if index.contains_key(start) {
            continue;
        }
        // each frame is a node and the children it has not visited yet
        let mut frames: Vec<(&str, Vec<&str>)> = Vec::new();
        index.insert(start, next);
        low.insert(start, next);
        next += 1;
        stack.push(start);
        on_stack.insert(start);
        frames.push((start, edges[start].iter().rev().copied().collect()));
        while let Some((v, pending)) = frames.last_mut() {
            let v = *v;
            if let Some(w) = pending.pop() {
                if !index.contains_key(w) {
                    index.insert(w, next);
                    low.insert(w, next);
                    next += 1;
                    stack.push(w);
                    on_stack.insert(w);
                    frames.push((w, edges[w].iter().rev().copied().collect()));
                } else if on_stack.contains(w) {
                    let lw = index[w];
                    let lv = low.get_mut(v).expect("visited");
                    *lv = (*lv).min(lw);
                }
                continue;
            }
            frames.pop();
            if let Some((parent, _)) = frames.last() {
                let lv = low[v];
                let lp = low.get_mut(parent).expect("visited");
                *lp = (*lp).min(lv);
            }
            if low[v] == index[v] {
                let mut comp = Vec::new();
                loop {
                    let w = stack.pop().expect("tarjan stack holds v");
                    on_stack.remove(w);
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                out.push(comp);
            }
        }
    }
    out
}
