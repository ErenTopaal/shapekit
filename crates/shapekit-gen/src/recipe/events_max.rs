//! `soroban.events-max`: contract events and return values at the size limit
//! core enforces per transaction, `txMaxContractEventsSizeBytes` from the
//! applied testnet settings (16384). Core adds the encoded size of every
//! contract event and then the return value, and fails the operation with
//! INVOKE_HOST_FUNCTION_RESOURCE_LIMIT_EXCEEDED once the sum passes the
//! limit (InvokeHostFunctionOpFrame.cpp, v30.0.0).
//!
//! Byte sizes are computed from the exact event the echo contract emits and
//! checked against the meta afterwards, so each entry sits on the limit to
//! the byte:
//!
//! - `echo(bytes)`: one event with the value as data, plus the same value
//!   returned, together exactly at the limit
//! - `emit(topics, bytes)`: one event with 200 topics and bytes data, plus
//!   the void return value, exactly at the limit
//! - each of the two with the data four bytes longer (the next XDR size),
//!   which core includes and fails

use anyhow::{Context, Result, ensure};
use ed25519_dalek::SigningKey;
use shapekit_core_driver::chain::Applied;
use shapekit_core_driver::settings::NetworkSettings;
use stellar_xdr::{
    ConfigSettingEntry, ConfigSettingId, ContractEvent, ContractEventBody, ContractEventType,
    ContractEventV0, ContractId, ExtensionPoint, InvokeHostFunctionResult, Limits, ScBytes,
    ScSymbol, ScVal, ScVec, TransactionMeta, WriteXdr,
};

use super::scval::{BALANCE, apply_failing, deploy_echo, invoke, invoke_failure, simulated_data};
use super::{ChainCtx, Recipe, Selector, Spec};

pub const FAMILY: &str = "soroban.events-max";

/// Topics of the many-topics event. The host puts no bound on the count; it
/// is chosen so the topics alone take a tenth of the limit.
const TOPICS: u32 = 200;

const ARGS: [&str; 1] = ["op.invoke_host_function.invoke_contract.args"];

pub struct EventsMax {
    pub protocol: u32,
}

fn xdr_len<T: WriteXdr>(v: &T) -> Result<usize> {
    Ok(v.to_xdr(Limits::none())?.len())
}

fn bytes_val(n: usize) -> Result<ScVal> {
    // a repeating, non-constant pattern, so a truncation shows
    let b: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
    Ok(ScVal::Bytes(ScBytes(b.try_into()?)))
}

fn event(contract: &ContractId, topics: Vec<ScVal>, data: ScVal) -> Result<ContractEvent> {
    Ok(ContractEvent {
        ext: ExtensionPoint::V0,
        contract_id: Some(contract.clone()),
        type_: ContractEventType::Contract,
        body: ContractEventBody::V0(ContractEventV0 {
            topics: topics.try_into()?,
            data,
        }),
    })
}

/// The event and return size core counts for an invocation that emits
/// `topics` with `data` and returns `ret`.
fn counted(contract: &ContractId, topics: &[ScVal], data: &ScVal, ret: &ScVal) -> Result<usize> {
    Ok(xdr_len(&event(contract, topics.to_vec(), data.clone())?)? + xdr_len(ret)?)
}

/// The largest bytes length `n` whose invocation counts at most `limit`
/// bytes, given how the count depends on `n`. Fails unless that count is
/// exactly `limit`.
fn fill(limit: usize, count: impl Fn(usize) -> Result<usize>) -> Result<usize> {
    let mut n = 0;
    while count(n + 1)? <= limit {
        n += 1;
    }
    ensure!(
        count(n)? == limit,
        "no bytes length reaches {limit} exactly; the closest is {}",
        count(n)?
    );
    Ok(n)
}

/// Events plus return value in the meta of a successful invocation.
fn meta_counted(applied: &Applied) -> Result<usize> {
    let TransactionMeta::V4(m) = &applied.meta else {
        anyhow::bail!("ledger {}: expected TransactionMeta v4", applied.ledger);
    };
    let mut n = 0;
    for op in m.operations.iter() {
        for e in op.events.iter() {
            n += xdr_len(e)?;
        }
    }
    let ret = applied
        .return_value()
        .context("a successful invocation without a return value")?;
    Ok(n + xdr_len(ret)?)
}

fn limit() -> Result<usize> {
    let settings = NetworkSettings::testnet()?;
    match settings.get(ConfigSettingId::ContractEventsV0) {
        Some(ConfigSettingEntry::ContractEventsV0(c)) => {
            Ok(c.tx_max_contract_events_size_bytes as usize)
        }
        other => anyhow::bail!("testnet settings have no contract events setting: {other:?}"),
    }
}

impl EventsMax {
    /// Records `function(args)` exactly at the limit and the twin with
    /// `over` instead, which core must fail for exceeding it.
    #[allow(clippy::too_many_arguments)]
    fn pair(
        ctx: &mut ChainCtx,
        caller: &SigningKey,
        echo: &ContractId,
        function: &str,
        args: Vec<ScVal>,
        over: Vec<ScVal>,
        slug: &str,
        what: &str,
        limit: usize,
    ) -> Result<()> {
        let data = simulated_data(ctx, caller, invoke(echo, function, over.clone())?)?;
        let applied = ctx.driver().invoke(caller, echo, function, args)?;
        let n = meta_counted(&applied)?;
        ensure!(
            n == limit,
            "{slug}: the meta counts {n} event and return bytes, expected {limit}"
        );
        ctx.capture(
            slug,
            Selector::ledger_of(&applied),
            Spec::new(format!(
                "{what}: events plus return value encode to {limit} bytes, exactly txMaxContractEventsSizeBytes"
            ))
            .fields(&ARGS)
            .flags(&["max-size"]),
        )?;
        // simulation does not check the event size, so the twin's own
        // simulated resources are used
        let twin = apply_failing(ctx, caller, invoke(echo, function, over)?, data, Vec::new())?;
        let r = invoke_failure(&twin)?;
        ensure!(
            matches!(r, InvokeHostFunctionResult::ResourceLimitExceeded),
            "{slug}: core answered {r:?} for an invocation over the event size limit"
        );
        ctx.capture(
            &format!("{slug}-over"),
            Selector::ledger_of(&twin),
            Spec::new(format!(
                "{what}, with data four bytes longer: {} bytes, which core includes and fails with invoke_host_function resource_limit_exceeded, so the events are discarded",
                limit + 4
            ))
            .fields(&ARGS),
        )
    }
}

impl Recipe for EventsMax {
    fn id(&self) -> &str {
        FAMILY
    }

    fn family(&self) -> &str {
        FAMILY
    }

    fn protocol(&self) -> u32 {
        self.protocol
    }

    fn soroban_settings(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut ChainCtx) -> Result<()> {
        let limit = limit()?;
        ctx.fund(&["caller"], BALANCE)?;
        let caller = ctx.account("caller");
        let echo = deploy_echo(ctx, &caller, FAMILY)?;

        // echo emits the value under the topic `last` and returns it
        let last = vec![ScVal::Symbol(ScSymbol("last".try_into()?))];
        let n = fill(limit, |n| {
            let v = bytes_val(n)?;
            counted(&echo, &last, &v, &v)
        })?;
        Self::pair(
            ctx,
            &caller,
            &echo,
            "echo",
            vec![bytes_val(n)?],
            vec![bytes_val(n + 4)?],
            "echo-bytes-at-limit",
            &format!("echo of {n} bytes, emitted as event data and returned"),
            limit,
        )?;

        let topics: Vec<ScVal> = (0..TOPICS).map(ScVal::U32).collect();
        let n = fill(limit, |n| {
            counted(&echo, &topics, &bytes_val(n)?, &ScVal::Void)
        })?;
        let topic_vec = ScVal::Vec(Some(ScVec(topics.clone().try_into()?)));
        Self::pair(
            ctx,
            &caller,
            &echo,
            "emit",
            vec![topic_vec.clone(), bytes_val(n)?],
            vec![topic_vec, bytes_val(n + 4)?],
            "emit-topics-bytes-at-limit",
            &format!("one event with {TOPICS} u32 topics and {n} bytes of data, void return"),
            limit,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::Hash;

    #[test]
    fn echo_fill_reaches_the_limit_exactly() {
        let c = ContractId(Hash([7; 32]));
        let last = vec![ScVal::Symbol(ScSymbol("last".try_into().unwrap()))];
        let n = fill(16384, |n| {
            let v = bytes_val(n)?;
            counted(&c, &last, &v, &v)
        })
        .unwrap();
        // ext, contract id, type, body arm, one symbol topic, then the
        // bytes twice (event data and return value)
        assert_eq!(n, 8152);
        let v = bytes_val(n + 4).unwrap();
        assert_eq!(counted(&c, &last, &v, &v).unwrap(), 16392);
    }

    #[test]
    fn fill_refuses_a_limit_it_cannot_hit() {
        let c = ContractId(Hash([7; 32]));
        let last = vec![ScVal::Symbol(ScSymbol("last".try_into().unwrap()))];
        // the echo count is 80 plus twice a multiple of four, so it is
        // always a multiple of eight
        let r = fill(16388, |n| {
            let v = bytes_val(n)?;
            counted(&c, &last, &v, &v)
        });
        assert!(r.is_err());
    }
}
