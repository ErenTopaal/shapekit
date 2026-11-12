//! Envelope-only facts: every address the transaction envelope names. These are what an
//! indexer would get if it read participants from the envelope, so they are reported next
//! to the meta-derived participants and never merged into them.

use stellar_xdr::{
    ContractIdPreimage, FeeBumpTransactionInnerTx, HostFunction, LedgerKey, MuxedAccount,
    Operation, OperationBody, RevokeSponsorshipOp, ScAddress, ScVal, SorobanAuthorizationEntry,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials,
    SorobanDelegateSignature, TransactionEnvelope, TransactionExt,
};

use crate::addr::{self, ParticipantSet};
use crate::vocab::{AddressKind, Declared};

pub fn declared(env: &TransactionEnvelope) -> Declared {
    let mut set = ParticipantSet::default();
    let (source, fee_source, ops, ext) = match env {
        TransactionEnvelope::TxV0(e) => {
            let src = MuxedAccount::Ed25519(e.tx.source_account_ed25519.clone());
            (src, None, &e.tx.operations, None)
        }
        TransactionEnvelope::Tx(e) => (
            e.tx.source_account.clone(),
            None,
            &e.tx.operations,
            Some(&e.tx.ext),
        ),
        TransactionEnvelope::TxFeeBump(e) => {
            let FeeBumpTransactionInnerTx::Tx(inner) = &e.tx.inner_tx;
            (
                inner.tx.source_account.clone(),
                Some(e.tx.fee_source.clone()),
                &inner.tx.operations,
                Some(&inner.tx.ext),
            )
        }
    };
    add_muxed(&mut set, &source, "tx-source");
    if let Some(f) = &fee_source {
        add_muxed(&mut set, f, "fee-source");
    }
    for op in ops.iter() {
        operation(&mut set, op);
    }
    if let Some(TransactionExt::V1(data)) = ext {
        let fp = &data.resources.footprint;
        for k in fp.read_only.iter().chain(fp.read_write.iter()) {
            footprint_key(&mut set, k);
        }
    }
    Declared {
        source: muxed_text(&source),
        fee_source: fee_source.as_ref().map(muxed_text),
        participants: set.into_vec(),
    }
}

// the source as written, so a muxed source stays visible as M... here
fn muxed_text(m: &MuxedAccount) -> String {
    m.to_string()
}

fn add_muxed(set: &mut ParticipantSet, m: &MuxedAccount, role: &str) {
    let (a, id) = addr::muxed(m);
    set.add(a, AddressKind::Account, id, role);
}

fn add_account(set: &mut ParticipantSet, a: &stellar_xdr::AccountId, role: &str) {
    set.add(addr::account(a), AddressKind::Account, None, role);
}

fn add_sc(set: &mut ParticipantSet, a: &ScAddress, role: &str) {
    let (s, kind, mux) = addr::sc_address(a);
    set.add(s, kind, mux, role);
}

fn operation(set: &mut ParticipantSet, op: &Operation) {
    if let Some(src) = &op.source_account {
        add_muxed(set, src, "op-source");
    }
    use OperationBody as B;
    match &op.body {
        B::CreateAccount(o) => add_account(set, &o.destination, "destination"),
        B::Payment(o) => add_muxed(set, &o.destination, "destination"),
        B::PathPaymentStrictReceive(o) => add_muxed(set, &o.destination, "destination"),
        B::PathPaymentStrictSend(o) => add_muxed(set, &o.destination, "destination"),
        B::AccountMerge(d) => add_muxed(set, d, "destination"),
        B::AllowTrust(o) => add_account(set, &o.trustor, "trustor"),
        B::SetTrustLineFlags(o) => add_account(set, &o.trustor, "trustor"),
        B::Clawback(o) => add_muxed(set, &o.from, "clawback-from"),
        B::SetOptions(o) => {
            if let Some(d) = &o.inflation_dest {
                add_account(set, d, "inflation-destination");
            }
        }
        B::CreateClaimableBalance(o) => {
            for c in o.claimants.iter() {
                let stellar_xdr::Claimant::ClaimantTypeV0(c) = c;
                add_account(set, &c.destination, "claimant");
            }
        }
        B::ClaimClaimableBalance(o) => set.add(
            addr::balance_id(&o.balance_id),
            AddressKind::ClaimableBalance,
            None,
            "claimable-balance",
        ),
        B::ClawbackClaimableBalance(o) => set.add(
            addr::balance_id(&o.balance_id),
            AddressKind::ClaimableBalance,
            None,
            "claimable-balance",
        ),
        B::BeginSponsoringFutureReserves(o) => add_account(set, &o.sponsored_id, "sponsored"),
        B::RevokeSponsorship(r) => match r {
            RevokeSponsorshipOp::LedgerEntry(k) => {
                if let Some((a, kind)) = addr::key_owner(k) {
                    set.add(a, kind, None, "revoke-sponsorship-target");
                }
            }
            RevokeSponsorshipOp::Signer(s) => {
                add_account(set, &s.account_id, "revoke-sponsorship-target")
            }
        },
        B::LiquidityPoolDeposit(o) => set.add(
            addr::pool(&o.liquidity_pool_id),
            AddressKind::LiquidityPool,
            None,
            "liquidity-pool",
        ),
        B::LiquidityPoolWithdraw(o) => set.add(
            addr::pool(&o.liquidity_pool_id),
            AddressKind::LiquidityPool,
            None,
            "liquidity-pool",
        ),
        B::InvokeHostFunction(o) => {
            host_function(set, &o.host_function);
            for a in o.auth.iter() {
                auth_entry(set, a);
            }
        }
        B::ManageSellOffer(_)
        | B::CreatePassiveSellOffer(_)
        | B::ManageBuyOffer(_)
        | B::Inflation
        | B::ManageData(_)
        | B::ChangeTrust(_)
        | B::BumpSequence(_)
        | B::EndSponsoringFutureReserves
        | B::ExtendFootprintTtl(_)
        | B::RestoreFootprint(_) => {}
    }
}

fn host_function(set: &mut ParticipantSet, f: &HostFunction) {
    match f {
        HostFunction::InvokeContract(a) => {
            add_sc(set, &a.contract_address, "invoked-contract");
            for v in a.args.iter() {
                scval_addresses(set, v, "argument");
            }
        }
        HostFunction::CreateContract(a) => preimage(set, &a.contract_id_preimage),
        HostFunction::CreateContractV2(a) => {
            preimage(set, &a.contract_id_preimage);
            for v in a.constructor_args.iter() {
                scval_addresses(set, v, "argument");
            }
        }
        HostFunction::UploadContractWasm(_) => {}
    }
}

fn preimage(set: &mut ParticipantSet, p: &ContractIdPreimage) {
    if let ContractIdPreimage::Address(a) = p {
        add_sc(set, &a.address, "deployer");
    }
}

fn auth_entry(set: &mut ParticipantSet, e: &SorobanAuthorizationEntry) {
    match &e.credentials {
        SorobanCredentials::SourceAccount => {}
        SorobanCredentials::Address(c) | SorobanCredentials::AddressV2(c) => {
            add_sc(set, &c.address, "auth-address")
        }
        SorobanCredentials::AddressWithDelegates(c) => {
            add_sc(set, &c.address_credentials.address, "auth-address");
            for d in c.delegates.iter() {
                delegate(set, d);
            }
        }
    }
    invocation(set, &e.root_invocation);
}

fn delegate(set: &mut ParticipantSet, d: &SorobanDelegateSignature) {
    add_sc(set, &d.address, "auth-delegate");
    for n in d.nested_delegates.iter() {
        delegate(set, n);
    }
}

fn invocation(set: &mut ParticipantSet, inv: &SorobanAuthorizedInvocation) {
    match &inv.function {
        SorobanAuthorizedFunction::ContractFn(a) => {
            add_sc(set, &a.contract_address, "auth-invocation");
            for v in a.args.iter() {
                scval_addresses(set, v, "argument");
            }
        }
        SorobanAuthorizedFunction::CreateContractHostFn(a) => {
            preimage(set, &a.contract_id_preimage)
        }
        SorobanAuthorizedFunction::CreateContractV2HostFn(a) => {
            preimage(set, &a.contract_id_preimage);
            for v in a.constructor_args.iter() {
                scval_addresses(set, v, "argument");
            }
        }
    }
    for s in inv.sub_invocations.iter() {
        invocation(set, s);
    }
}

fn footprint_key(set: &mut ParticipantSet, k: &LedgerKey) {
    if let Some((a, kind)) = addr::key_owner(k) {
        set.add(a, kind, None, "footprint");
    }
}

fn scval_addresses(set: &mut ParticipantSet, v: &ScVal, role: &str) {
    match v {
        ScVal::Address(a) => add_sc(set, a, role),
        ScVal::Vec(Some(items)) => {
            for i in items.0.iter() {
                scval_addresses(set, i, role);
            }
        }
        ScVal::Map(Some(m)) => {
            for e in m.0.iter() {
                scval_addresses(set, &e.key, role);
                scval_addresses(set, &e.val, role);
            }
        }
        _ => {}
    }
}
