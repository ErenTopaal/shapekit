//! Recursion-depth vectors: for each recursive group, values nested so deep that a decoder's
//! depth limit decides them. RFC 4506 sets no depth limit, so the interpreter cannot say where
//! the boundary is; the limit and the way levels are counted belong to the decoder that applies
//! them. The caller passes that count in as a measuring function, and the builder picks the
//! nesting that lands one level below the limit, on it, and one above it.

use crate::interp::Value;
use crate::names::rust_type_name;
use crate::vectors::{Builder, Expect, Vector, VectorError};
use crate::{Definition, Type};

/// Where a vector sits against the limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Below,
    At,
    Above,
}

impl Side {
    pub const ALL: [Side; 3] = [Side::Below, Side::At, Side::Above];

    pub fn as_str(self) -> &'static str {
        match self {
            Side::Below => "below",
            Side::At => "at",
            Side::Above => "above",
        }
    }

    fn offset(self) -> i64 {
        match self {
            Side::Below => -1,
            Side::At => 0,
            Side::Above => 1,
        }
    }
}

/// One depth vector and what was measured on it.
#[derive(Clone, Debug)]
pub struct DepthVector {
    pub vector: Vector,
    pub side: Side,
    /// The group member the nesting repeats, by its XDR name.
    pub entry: String,
    /// The depth the measuring function reports for the bytes.
    pub depth: u32,
    /// The most nested types by the interpreter's own count.
    pub interp_depth: u32,
    /// Times the group's cycle is repeated around the innermost value.
    pub repeats: usize,
    /// Every definition on the way from the root to the entry and around the cycle, by XDR
    /// name.
    pub types: Vec<String>,
}

/// One step around a cycle: the outer definition, the definition it holds and which of its
/// components holds it.
type Hop = (String, String, usize);

/// Returns the levels a decoder needs for a value: the type's stellar-xdr crate name and the
/// bytes in, the count out.
pub type Measure<'m> = dyn FnMut(&str, &[u8]) -> Result<u32, String> + 'm;

/// Most variants kept per definition and level when looking for seeds.
const VARIANTS: usize = 64;

impl Builder<'_> {
    /// A value of type `ty`, written inline, holding `inner` where `ty` names `target` without
    /// passing through another definition; every other part is its smallest value.
    fn place(&self, ty: &Type, target: &str, inner: &Value) -> Option<Value> {
        match ty {
            Type::Ref { name } if name == target => Some(inner.clone()),
            Type::Optional { element } => Some(Value::Optional(Some(Box::new(
                self.place(element, target, inner)?,
            )))),
            Type::VarArray { element, max_count } if *max_count != Some(0) => {
                Some(Value::Array(vec![self.place(element, target, inner)?]))
            }
            Type::Array { element, count } if *count > 0 => {
                let first = self.place(element, target, inner)?;
                let rest = self.smallest_type(element)?;
                let mut items = vec![first];
                items.extend(std::iter::repeat_n(rest, *count as usize - 1));
                Some(Value::Array(items))
            }
            _ => None,
        }
    }

    /// The smallest values of definition `outer` that hold `inner` as a value of `target`, one
    /// per component that can: each struct field, each union arm (its first case) or the
    /// typedef's type, in schema order.
    fn embed_all(&self, outer: &str, target: &str, inner: &Value) -> Vec<Value> {
        match self.schema.get(outer) {
            Some(Definition::Typedef(t)) => self.place(&t.ty, target, inner).into_iter().collect(),
            Some(Definition::Struct(s)) => s
                .fields
                .iter()
                .enumerate()
                .filter_map(|(i, f)| {
                    let placed = self.place(&f.ty, target, inner)?;
                    let Some(Value::Struct(mut fields)) = self.smallest(outer).cloned() else {
                        return None;
                    };
                    fields[i] = placed;
                    Some(Value::Struct(fields))
                })
                .collect(),
            Some(Definition::Union(u)) => u
                .arms
                .iter()
                .filter_map(|a| {
                    let placed = self.place(a.ty.as_ref()?, target, inner)?;
                    Some(Value::Union {
                        discriminant: a.cases.first()?.value,
                        arm: Some(Box::new(placed)),
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// [`Self::embed_all`] through the component numbered `slot`.
    fn embed_at(&self, outer: &str, target: &str, slot: usize, inner: &Value) -> Option<Value> {
        self.embed_all(outer, target, inner).into_iter().nth(slot)
    }

    /// The first of [`Self::embed_all`].
    fn embed(&self, outer: &str, target: &str, inner: &Value) -> Option<Value> {
        self.embed_at(outer, target, 0, inner)
    }

    /// How many components of `outer` can hold `target`.
    fn slots(&self, outer: &str, target: &str) -> usize {
        self.smallest(target)
            .map_or(0, |v| self.embed_all(outer, target, v).len())
    }

    /// One way around a recursive group: a cycle of definitions from the entry back to it,
    /// with the component taken at each step.
    fn wrappers(&self, group: &[&str], entry: &str) -> Vec<Vec<Hop>> {
        // simple cycles through the entry inside the group, then every choice of component
        let mut cycles: Vec<Vec<String>> = Vec::new();
        let mut stack: Vec<(Vec<String>, usize)> = vec![(vec![entry.to_string()], 0)];
        let graph = self.schema.graph();
        while let Some((path, _)) = stack.pop() {
            let last = path.last().expect("a path is never empty").clone();
            let mut next: Vec<&str> = graph
                .children(&last)
                .filter(|c| group.contains(c) && self.slots(&last, c) > 0)
                .collect();
            next.reverse();
            for c in next {
                if c == entry {
                    let mut cyc = path.clone();
                    cyc.push(c.to_string());
                    cycles.push(cyc);
                } else if !path.iter().any(|p| p == c) {
                    let mut p = path.clone();
                    p.push(c.to_string());
                    stack.push((p, 0));
                }
            }
        }
        cycles.sort_by_key(|c| c.len());
        let mut out = Vec::new();
        for cyc in cycles {
            let hops: Vec<Hop> = cyc
                .windows(2)
                .map(|w| (w[0].clone(), w[1].clone(), self.slots(&w[0], &w[1])))
                .collect();
            // every combination of components, counting through them like digits
            let mut pick = vec![0usize; hops.len()];
            loop {
                out.push(
                    hops.iter()
                        .zip(&pick)
                        .map(|((o, t, _), s)| (o.clone(), t.clone(), *s))
                        .collect(),
                );
                let mut i = 0;
                while i < pick.len() {
                    pick[i] += 1;
                    if pick[i] < hops[i].2 {
                        break;
                    }
                    pick[i] = 0;
                    i += 1;
                }
                if i == pick.len() || out.len() >= 256 {
                    break;
                }
            }
            if out.len() >= 256 {
                break;
            }
        }
        out
    }

    /// `inner`, a value of the entry, wrapped once around the cycle `w`.
    fn wrap_once(&self, w: &[Hop], inner: Value) -> Option<Value> {
        let mut v = inner;
        for (outer, target, slot) in w.iter().rev() {
            v = self.embed_at(outer, target, *slot, &v)?;
        }
        Some(v)
    }

    /// Whether `outer` can hold `target` directly.
    fn holds(&self, outer: &str, target: &str) -> bool {
        self.smallest(target)
            .and_then(|v| self.embed(outer, target, v))
            .is_some()
    }

    /// The shortest chain of definitions from `from` to `to`, both included, each holding the
    /// next directly. With `from == to` it is the shortest cycle, at least one step long.
    fn chain(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let graph = self.schema.graph();
        let mut prev: std::collections::BTreeMap<&str, &str> = Default::default();
        let mut queue = std::collections::VecDeque::new();
        for c in graph.children(from) {
            if self.holds(from, c) && !prev.contains_key(c) {
                prev.insert(c, from);
                queue.push_back(c);
            }
        }
        while let Some(n) = queue.pop_front() {
            if n == to {
                let mut out = vec![to.to_string()];
                let mut at = to;
                loop {
                    let p = prev[at];
                    out.push(p.to_string());
                    if p == from {
                        break;
                    }
                    at = p;
                }
                out.reverse();
                return Some(out);
            }
            for c in graph.children(n) {
                if self.holds(n, c) && !prev.contains_key(c) {
                    prev.insert(c, n);
                    queue.push_back(c);
                }
            }
        }
        None
    }

    /// `inner`, a value of the chain's last definition, wrapped into a value of its first.
    fn wrap(&self, chain: &[String], inner: Value) -> Option<Value> {
        let mut v = inner;
        for w in chain.windows(2).rev() {
            v = self.embed(&w[0], &w[1], &v)?;
        }
        Some(v)
    }

    /// The member of `group` most definitions outside the group refer to, ties by name: the
    /// way into the group a decoder usually takes.
    pub fn group_entry(&self, group: &[&str]) -> String {
        let graph = self.schema.graph();
        let incoming = |m: &str| {
            self.schema
                .definitions()
                .iter()
                .filter(|d| !group.contains(&d.name()) && graph.children(d.name()).any(|c| c == m))
                .count()
        };
        let mut best = group[0];
        for m in group {
            if incoming(m) > incoming(best) {
                best = m;
            }
        }
        best.to_string()
    }

    /// Values of `ty` that differ from its smallest in one place at each of `level` levels:
    /// another union arm, an optional present, an array with an element. Their encodings
    /// nest to different depths.
    fn type_variants(&self, ty: &Type, level: u32) -> Vec<Value> {
        let Some(small) = self.smallest_type(ty) else {
            return Vec::new();
        };
        let mut out = vec![small];
        if level == 0 {
            return out;
        }
        match ty {
            Type::Ref { name } => out.extend(self.definition_variants(name, level)),
            Type::Optional { element } => out.extend(
                self.type_variants(element, level - 1)
                    .into_iter()
                    .map(|v| Value::Optional(Some(Box::new(v)))),
            ),
            Type::VarArray { element, max_count } if *max_count != Some(0) => out.extend(
                self.type_variants(element, level - 1)
                    .into_iter()
                    .map(|v| Value::Array(vec![v])),
            ),
            _ => {}
        }
        out
    }

    fn definition_variants(&self, name: &str, level: u32) -> Vec<Value> {
        let mut out: Vec<Value> = self.smallest(name).cloned().into_iter().collect();
        if level == 0 {
            return out;
        }
        match self.schema.get(name) {
            Some(Definition::Typedef(t)) => out.extend(self.type_variants(&t.ty, level - 1)),
            Some(Definition::Union(u)) => {
                for a in &u.arms {
                    for c in &a.cases {
                        match &a.ty {
                            None => out.push(Value::Union {
                                discriminant: c.value,
                                arm: None,
                            }),
                            Some(t) => {
                                out.extend(self.type_variants(t, level - 1).into_iter().map(|v| {
                                    Value::Union {
                                        discriminant: c.value,
                                        arm: Some(Box::new(v)),
                                    }
                                }))
                            }
                        }
                    }
                }
            }
            Some(Definition::Struct(s)) => {
                if let Some(Value::Struct(fields)) = self.smallest(name) {
                    for (i, f) in s.fields.iter().enumerate() {
                        for v in self.type_variants(&f.ty, level - 1) {
                            let mut fs = fields.clone();
                            fs[i] = v;
                            out.push(Value::Struct(fs));
                        }
                    }
                }
            }
            _ => {}
        }
        let mut seen = Vec::new();
        out.retain(|v| {
            let fresh = !seen.contains(v);
            if fresh {
                seen.push(v.clone());
            }
            fresh
        });
        // enough to cover every depth remainder; wide definitions would otherwise multiply
        out.truncate(VARIANTS);
        out
    }

    /// The innermost values a nesting can start from: values of each definition on the cycle
    /// that differ from its smallest a few levels down, carried up to the entry. Their depths
    /// differ, which lets a nesting land on every depth and not only on the multiples of the
    /// cycle's length.
    fn seeds(&self, cycle: &[String]) -> Vec<Value> {
        let mut out: Vec<Value> = Vec::new();
        for j in 0..cycle.len() - 1 {
            for v in self.definition_variants(&cycle[j], 6) {
                if let Some(w) = self.wrap(&cycle[..=j], v)
                    && !out.contains(&w)
                {
                    out.push(w);
                }
            }
        }
        out
    }

    /// Depth vectors for one recursive group inside `root` (a group member or a definition
    /// that reaches the group) under a decoder that allows `limit` levels: one value at
    /// `limit - 1`, one at `limit` and one at `limit + 1` levels by `measure`'s count.
    /// `measure` gets the root's stellar-xdr crate name and the bytes and returns the fewest
    /// levels a decode of them needs. The two values within the limit must be accepted; the
    /// one past it is implementation-defined, because decoders count levels differently.
    pub fn depth_vectors(
        &self,
        group: &[&str],
        root: &str,
        limit: u32,
        measure: &mut Measure<'_>,
    ) -> Result<Vec<DepthVector>, VectorError> {
        let entry = self.group_entry(group);
        let fail = |why: String| VectorError::Depth {
            group: entry.clone(),
            root: root.to_string(),
            why,
        };
        let cycle = self
            .chain(&entry, &entry)
            .ok_or_else(|| fail("no cycle through the entry".into()))?;
        let context = if root == entry {
            vec![entry.clone()]
        } else {
            self.chain(root, &entry)
                .ok_or_else(|| fail(format!("{root} does not reach {entry}")))?
        };
        let root_rust = rust_type_name(root);
        let entry_rust = rust_type_name(&entry);
        let seeds = self.seeds(&cycle);
        let wrappers = self.wrappers(group, &entry);
        let primary = wrappers
            .first()
            .ok_or_else(|| fail("no cycle through the entry".into()))?
            .clone();
        let mut depth_of = |bytes: &[u8]| measure(&root_rust, bytes).map_err(fail);

        // `plan` lists the wrappers to apply around the seed, innermost first
        let build = |seed: &Value, plan: &[&[Hop]]| -> Result<Vec<u8>, VectorError> {
            let mut v = seed.clone();
            for w in plan {
                v = self
                    .wrap_once(w, v)
                    .ok_or_else(|| fail("the cycle does not wrap".into()))?;
            }
            let v = self
                .wrap(&context, v)
                .ok_or_else(|| fail("the context does not wrap".into()))?;
            self.encode(root, &v)
        };
        // a few turns of the primary cycle first, so the nesting is the deepest part of the
        // value and every further turn adds its whole length
        const LEAD: usize = 4;
        let lead: Vec<&[Hop]> = vec![primary.as_slice(); LEAD];
        let seed0 = seeds.first().ok_or_else(|| fail("no seed value".into()))?;
        let base = depth_of(&build(seed0, &lead)?)?;
        let step_of = |w: &[Hop],
                       depth_of: &mut dyn FnMut(&[u8]) -> Result<u32, VectorError>|
         -> Result<u32, VectorError> {
            let mut plan = lead.clone();
            plan.push(w);
            Ok(depth_of(&build(seed0, &plan)?)?.saturating_sub(base))
        };
        let p = step_of(&primary, &mut depth_of)?;
        if p == 0 {
            return Err(fail("a turn of the cycle adds no depth".into()));
        }
        // the other cycles, one per step length
        let mut others: Vec<(&[Hop], u32)> = Vec::new();
        for w in &wrappers[1..] {
            let s = step_of(w, &mut depth_of)?;
            if s > 0 && s != p && !others.iter().any(|(_, o)| *o == s) {
                others.push((w, s));
            }
        }

        let mut out = Vec::new();
        for side in Side::ALL {
            let want = (i64::from(limit) + side.offset()) as u32;
            let mut found = None;
            'seeds: for seed in &seeds {
                let d = depth_of(&build(seed, &lead)?)?;
                let Some(rest) = want.checked_sub(d) else {
                    continue;
                };
                // `m` turns of another cycle fix the remainder modulo the primary step
                let choices =
                    std::iter::once((primary.as_slice(), p)).chain(others.iter().copied());
                for (w, s) in choices {
                    for m in 0..p {
                        let Some(left) = rest.checked_sub(m * s) else {
                            break;
                        };
                        if left % p != 0 {
                            continue;
                        }
                        let n = (left / p) as usize;
                        let mut plan = lead.clone();
                        plan.extend(std::iter::repeat_n(w, m as usize));
                        plan.extend(std::iter::repeat_n(primary.as_slice(), n));
                        let bytes = build(seed, &plan)?;
                        if depth_of(&bytes)? == want {
                            found = Some((bytes, plan.len()));
                            break 'seeds;
                        }
                    }
                }
            }
            let (bytes, repeats) = found.ok_or_else(|| {
                fail(format!(
                    "no plan over {} seeds and {} cycle lengths lands on depth {want}",
                    seeds.len(),
                    others.len() + 1
                ))
            })?;
            // the interpreter has no depth limit of its own to apply here; it only counts
            let mut counter = crate::interp::Interp::new(self.schema);
            counter.depth_limit = u32::MAX;
            let (_, interp_depth) = counter
                .decode_nesting(root, &bytes)
                .map_err(|e| fail(format!("the interpreter refuses it: {e}")))?;
            let expect = match side {
                Side::Below | Side::At => Expect::MustAccept,
                Side::Above => Expect::ImplementationDefined,
            };
            let point = format!("#depth-{entry_rust}");
            out.push(DepthVector {
                vector: Vector {
                    obligation: format!("format:{root_rust}{point}:{}", side.as_str()),
                    type_name: root_rust.clone(),
                    bytes,
                    expect,
                    point,
                    note: format!(
                        "{entry_rust} holding itself {repeats} times, {want} levels deep by the measured count against a limit of {limit} ({interp_depth} levels by the interpreter's count)"
                    ),
                },
                side,
                entry: entry.clone(),
                depth: want,
                interp_depth,
                repeats,
                types: context.iter().chain(&cycle).cloned().collect(),
            });
        }
        Ok(out)
    }
}
