//! One episode: a fresh chain, a few funded roles, then a sequence of filled steps run on
//! core one at a time. Each step's outcome is learned from core and written into the recipe
//! as its expectation, so the recipe the episode leaves behind replays the same chain.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use shapekit_contracts::RecipeIr;
use shapekit_contracts::recipe_ir::{NodeOptions, Outcome, Step, TxExpect};
use shapekit_core_driver::config::CoreImage;
use shapekit_gen::recipe::ir_run::{IrRecipe, Session};
use shapekit_gen::recipe::{ChainCtx, Network, start_chain};
use shapekit_schema::Schema;
use shapekit_twin::coverage::{Counts, merge_and_export, read_export};

use crate::action::{self, OpMenu};
use crate::arms::{arms_in, ledger_signature};
use crate::feedback::Seen;
use crate::rng::Rng;
use crate::world::World;

/// The family explorer recipes name. They capture no entries, so no registry family is
/// looked up for them.
pub const FAMILY: &str = "explore";

/// Roles a fresh episode funds richly, the first two of which issue the assets.
pub const RICH_ROLES: &[&str] = &["a", "b", "c", "d"];
/// The role a fresh episode funds near the reserve.
pub const POOR_ROLE: &str = "e";
pub const RICH_BALANCE: &str = "100000000000";

/// What stays the same for every episode of a run.
pub struct Env<'a> {
    pub schema: &'a Schema,
    pub image: &'a CoreImage,
    /// Measure core coverage: `image` is the coverage twin and every episode's profile is
    /// merged and read.
    pub coverage: bool,
    /// The repository root, which upload paths are relative to.
    pub root: &'a Path,
    pub reachable: &'a BTreeSet<String>,
    /// Where node work directories and profiles go.
    pub work: &'a Path,
    pub seen: &'a Seen,
}

#[derive(Debug, Clone)]
pub struct Spec {
    pub id: String,
    pub protocol: u32,
    /// Protocol upgrades stop here.
    pub max_protocol: u32,
    pub node: NodeOptions,
    /// Steps run as they are before any new step: a replay, or a kept recipe to extend.
    pub prefix: Vec<Step>,
    /// New steps to draw after the prefix.
    pub actions: usize,
    pub contracts: bool,
    pub seed: u64,
}

impl Spec {
    pub fn ir(&self, steps: Vec<Step>) -> RecipeIr {
        RecipeIr {
            schema_version: 1,
            id: self.id.clone(),
            family: FAMILY.into(),
            protocol: self.protocol,
            node: self.node.clone(),
            below_protocol_valid: false,
            steps,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub applied: usize,
    pub failed: usize,
    pub rejected: usize,
    /// Filled values the stellar-xdr decoder refused; a filler defect when nonzero.
    pub invalid_fills: usize,
    pub upgrades_refused: usize,
    /// Queued, then left out of the ledger by core.
    pub left_out: usize,
    /// Refusals at submission by core's answer.
    pub refusals: BTreeMap<String, usize>,
}

impl Stats {
    pub fn add(&mut self, o: &Stats) {
        self.applied += o.applied;
        self.failed += o.failed;
        self.rejected += o.rejected;
        self.invalid_fills += o.invalid_fills;
        self.upgrades_refused += o.upgrades_refused;
        self.left_out += o.left_out;
        for (k, v) in &o.refusals {
            *self.refusals.entry(k.clone()).or_insert(0) += v;
        }
    }
}

/// A short name for a refusal: the transaction code, and for `tx_failed` the first
/// operation code that is not a success.
fn refusal_code(r: &stellar_xdr::TransactionResultResult) -> String {
    let v = serde_json::to_value(r).unwrap_or_default();
    match &v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(m) => {
            let (k, inner) = m.iter().next().expect("a union arm");
            let first = inner.as_array().and_then(|ops| {
                ops.iter()
                    .map(|op| op.to_string())
                    .find(|t| !t.contains("success"))
            });
            match first {
                Some(op) => format!("{k} {op}"),
                None => k.clone(),
            }
        }
        _ => v.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub ir: RecipeIr,
    /// Every reachable arm the chain's ledgers hold, with the first ledger holding it.
    pub arms: BTreeMap<String, u32>,
    /// Steps whose ledger held a signature path no earlier ledger of the run held.
    pub novel_steps: BTreeSet<usize>,
    pub new_paths: usize,
    /// Core's region counts, when measured on the twin.
    pub counts: Option<Counts>,
    pub prefix_len: usize,
    pub stats: Stats,
    pub seconds: f64,
}

struct Live<'e, 'a> {
    env: &'e Env<'a>,
    ctx: ChainCtx,
    session: Session<'a>,
    steps: Vec<Step>,
    world: World,
    arms: BTreeMap<String, u32>,
    novel: BTreeSet<usize>,
    new_paths: usize,
    stats: Stats,
    menu_credit: Vec<(Vec<String>, u64)>,
}

/// What a submitted transaction came to.
enum Pushed {
    /// Applied in the ledger that closed; how many new paths that ledger held.
    Applied(usize),
    Rejected,
    Invalid,
}

fn is_fill_error(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        let s = c.to_string();
        s.contains("is not valid XDR-JSON")
            || s.contains("overflows")
            || s.contains("cannot grow")
            // TryAgainLater, Duplicate or Filtered: core neither queued nor judged it
            || s.contains("to the submission")
            || s.contains("a fee bump wraps a v1 envelope")
            // the source was merged away earlier in the episode
            || (s.starts_with("account ") && s.ends_with(" does not exist"))
    })
}

impl<'e, 'a> Live<'e, 'a> {
    /// Takes in the ledger that just closed. The step at `index` closed it.
    fn closed(&mut self, index: usize) -> Result<usize> {
        let seq = self.ctx.driver().last_closed();
        let lcm = self.ctx.driver().last_meta().clone();
        let sig = ledger_signature(&lcm)?;
        for a in arms_in(&sig, self.env.reachable) {
            self.arms.entry(a).or_insert(seq);
        }
        let new = self.env.seen.paths(&sig).len();
        if new > 0 {
            self.novel.insert(index);
            self.new_paths += new;
        }
        self.world.observe(&lcm, seq)?;
        Ok(new)
    }

    /// Runs `step` as it is, appended to the recipe. A transaction step core refuses at
    /// submission is dropped from the recipe.
    fn exact(&mut self, step: Step) -> Result<()> {
        let i = self.steps.len();
        let before = self.ctx.driver().last_closed();
        self.steps.push(step.clone());
        match self.session.step(&mut self.ctx, i, &step) {
            Ok(()) => {}
            Err(e) if matches!(step, Step::Tx { .. }) && is_fill_error(&e) => {
                self.steps.pop();
                self.stats.invalid_fills += 1;
                return Ok(());
            }
            Err(e) if matches!(step, Step::Upgrade { .. }) => {
                self.stats.upgrades_refused += 1;
                if self.ctx.driver().last_closed() == before {
                    // nothing was armed or closed
                    self.steps.pop();
                    return Ok(());
                }
                // the ledger closed without the upgrade: a plain close in the recipe
                let _ = e;
                self.steps[i] = Step::Close { label: None };
                self.session.take_observed();
                return self.closed(i).map(|_| ());
            }
            Err(e) => return Err(e),
        }
        self.learn()?;
        if matches!(step, Step::Tx { .. }) && self.ctx.driver().last_closed() == before {
            // refused (dropped by learn) or queued for the next close
            return Ok(());
        }
        if self.ctx.driver().last_closed() != before {
            // a close or an upgrade, the last step even if learning dropped earlier ones
            self.closed(self.steps.len() - 1)?;
        }
        Ok(())
    }

    /// Writes what core answered into the steps it answered for; drops refused ones.
    fn learn(&mut self) -> Result<()> {
        let mut drop = self.session.take_left_out();
        self.stats.left_out += drop.len();
        for o in self.session.take_observed() {
            let fee_bump = matches!(
                &self.steps[o.step],
                Step::Tx {
                    fee_bump: Some(_),
                    ..
                }
            );
            let result = if fee_bump {
                // the inner result names the inner transaction's hash, which differs per
                // network passphrase
                None
            } else {
                Some(
                    self.world
                        .recipe_form(serde_json::to_value(&o.result.result)?),
                )
            };
            match o.outcome {
                Outcome::Rejected => {
                    self.stats.rejected += 1;
                    *self
                        .stats
                        .refusals
                        .entry(refusal_code(&o.result.result))
                        .or_insert(0) += 1;
                    drop.push(o.step);
                }
                Outcome::Success => self.stats.applied += 1,
                Outcome::Failed => self.stats.failed += 1,
            }
            match &mut self.steps[o.step] {
                Step::Tx { expect, .. } | Step::Invoke { expect, .. } => {
                    *expect = TxExpect {
                        outcome: o.outcome,
                        result,
                    }
                }
                Step::Fund { .. } => {
                    if o.outcome != Outcome::Success {
                        bail!(
                            "funding the roles failed: {}",
                            serde_json::to_string(&o.result.result)?
                        );
                    }
                }
                other => bail!("core answered for a step that submits nothing: {other:?}"),
            }
        }
        drop.sort_unstable();
        for i in drop.into_iter().rev() {
            self.steps.remove(i);
        }
        Ok(())
    }

    /// Submits a new transaction step and closes the ledger it goes into.
    fn transaction(&mut self, step: Step) -> Result<Pushed> {
        let before = self.steps.len();
        let rejected = self.stats.rejected;
        let invalid = self.stats.invalid_fills;
        self.exact(step)?;
        if self.stats.invalid_fills > invalid {
            return Ok(Pushed::Invalid);
        }
        if self.stats.rejected > rejected || self.steps.len() == before {
            return Ok(Pushed::Rejected);
        }
        let i = self.steps.len();
        self.steps.push(Step::Close { label: None });
        self.session
            .step(&mut self.ctx, i, &Step::Close { label: None })?;
        let n = self.steps.len();
        self.learn()?;
        let new = self.closed(self.steps.len() - 1)?;
        if self.steps.len() < n {
            // core left it out of the ledger
            return Ok(Pushed::Rejected);
        }
        Ok(Pushed::Applied(new))
    }
}

fn funded_roles(steps: &[Step]) -> Vec<String> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::Fund { roles, .. } => Some(roles.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The steps a fresh episode starts with.
pub fn setup(rng: &mut Rng) -> Vec<Step> {
    // the genesis base reserve is 10 XLM, so an account needs 20 XLM to exist
    let poor = *rng.pick(&["200000000", "210000000", "250000000"]);
    vec![
        Step::Fund {
            label: None,
            roles: RICH_ROLES.iter().map(|r| (*r).to_owned()).collect(),
            balance: RICH_BALANCE.into(),
        },
        // core queues one transaction per source account at a time
        Step::Close { label: None },
        Step::Fund {
            label: None,
            roles: vec![POOR_ROLE.into()],
            balance: poor.into(),
        },
        Step::Close { label: None },
    ]
}

/// Runs one episode. `menu` is updated with the novelty each operation arm earned.
pub fn run(env: &Env<'_>, spec: &Spec, menu: &mut OpMenu) -> Result<Episode> {
    let started = Instant::now();
    let mut rng = Rng::new(spec.seed);
    let work = tempfile::Builder::new()
        .prefix("explore-node-")
        .tempdir_in(env.work)
        .context("creating the node workdir")?;
    let mut image = env.image.clone();
    let profiles = if env.coverage {
        let p: PathBuf = work.path().join("profiles");
        std::fs::create_dir_all(&p)?;
        image.profile_dir = Some(p.canonicalize()?);
        Some(p)
    } else {
        None
    };
    let node_dir = work.path().join("node");
    std::fs::create_dir_all(&node_dir)?;
    let shell = IrRecipe::new(spec.ir(Vec::new()), env.root);
    let mut ctx = start_chain(&shell, Network::Testnet, &image, &node_dir)?;
    let mut session = Session::new(&mut ctx, env.root, spec.node.soroban_settings)?;
    session.set_learning(true);
    let mut world = World::new(&spec.id, Network::Testnet.passphrase(), spec.protocol);
    world.last_closed = ctx.driver().last_closed();
    let mut live = Live {
        env,
        ctx,
        session,
        steps: Vec::new(),
        world,
        arms: BTreeMap::new(),
        novel: BTreeSet::new(),
        new_paths: 0,
        stats: Stats::default(),
        menu_credit: Vec::new(),
    };
    let prefix = if spec.prefix.is_empty() {
        setup(&mut rng)
    } else {
        spec.prefix.clone()
    };
    for r in funded_roles(&prefix) {
        live.world.add_role(&r);
    }
    for r in RICH_ROLES.iter().take(2) {
        live.world.issuers.push((*r).to_owned());
    }
    let outcome = (|| -> Result<()> {
        for step in prefix {
            live.exact(step)?;
        }
        Ok(())
    })();
    let prefix_len = live.steps.len();
    let outcome = outcome.and_then(|()| {
        for _ in 0..spec.actions {
            if live.world.funded.is_empty() {
                // every role was merged away
                break;
            }
            if rng.chance(1, 25) {
                let s = action::upgrade(&live.world, &mut rng, spec.max_protocol);
                let protocol = match &s {
                    Step::Upgrade {
                        protocol_version: Some(v),
                        ..
                    } => Some(*v),
                    _ => None,
                };
                let before = live.steps.len();
                live.exact(s)?;
                if let Some(v) = protocol
                    && live.steps.len() > before
                    && matches!(live.steps[before], Step::Upgrade { .. })
                {
                    live.world.protocol = v;
                }
                continue;
            }
            let (s, arms) = action::transaction(env.schema, &live.world, &mut rng, menu);
            match live.transaction(s)? {
                Pushed::Applied(new) => live.menu_credit.push((arms, new as u64)),
                Pushed::Rejected | Pushed::Invalid => {}
            }
        }
        Ok(())
    });
    let Live {
        ctx,
        steps,
        arms,
        novel,
        new_paths,
        stats,
        menu_credit,
        ..
    } = live;
    let stopped = ctx.stop();
    outcome?;
    stopped?;
    for (arms, n) in menu_credit {
        for a in arms {
            menu.credit(&a, n);
        }
    }
    let counts = match &profiles {
        Some(p) => Some(read_export(&merge_and_export(env.image, p)?)?),
        None => None,
    };
    Ok(Episode {
        ir: spec.ir(steps),
        arms,
        novel_steps: novel,
        new_paths,
        counts,
        prefix_len,
        stats,
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// The steps of `ep` with only its setup and the steps that showed something new, each
/// transaction with the close after it.
pub fn shrunk_steps(ep: &Episode) -> Vec<Step> {
    let mut out: Vec<Step> = ep.ir.steps[..ep.prefix_len].to_vec();
    let steps = &ep.ir.steps;
    let mut i = ep.prefix_len;
    while i < steps.len() {
        match &steps[i] {
            Step::Tx { .. } | Step::Invoke { .. } => {
                // a new transaction is always followed by the close that applied it
                if ep.novel_steps.contains(&(i + 1)) {
                    out.push(steps[i].clone());
                    out.push(steps[i + 1].clone());
                }
                i += 2;
            }
            other => {
                if ep.novel_steps.contains(&i) {
                    out.push(other.clone());
                }
                i += 1;
            }
        }
    }
    out
}
