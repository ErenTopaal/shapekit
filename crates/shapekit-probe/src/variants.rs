//! The single-field changes of the base payment.

use shapekit_core_driver::keys::muxed_account_with_id;
use stellar_xdr::{
    Hash, Memo, MuxedAccount, MuxedAccountMed25519, OperationBody, Preconditions, SequenceNumber,
    TimeBounds, TimePoint, Uint256,
};

use crate::{Ctx, Draft, Variant};

fn payment_destination(d: &mut Draft, to: MuxedAccount) {
    let mut ops = d.tx.operations.to_vec();
    match &mut ops[0].body {
        OperationBody::Payment(p) => p.destination = to,
        _ => unreachable!("the base transaction is one payment"),
    }
    d.tx.operations = ops
        .try_into()
        .expect("the same number of operations fits again");
}

/// The variants of the first measurement: memo arms, muxed ids of source and destination, and
/// four values core must refuse (sequence number, fee, time bound, signature).
pub fn payment() -> Vec<Variant> {
    vec![
        Variant {
            field: "tx.memo.type",
            name: "base payment, no memo",
            edit: |_, _| {},
        },
        Variant {
            field: "tx.memo.id",
            name: "memo id 2^64-1",
            edit: |d, _| d.tx.memo = Memo::Id(u64::MAX),
        },
        Variant {
            field: "tx.memo.hash",
            name: "memo hash of 32 x 0xff",
            edit: |d, _| d.tx.memo = Memo::Hash(Hash([0xff; 32])),
        },
        Variant {
            field: "tx.memo.return",
            name: "memo return of 32 x 0x00",
            edit: |d, _| d.tx.memo = Memo::Return(Hash([0; 32])),
        },
        Variant {
            field: "op.payment.destination.mux_id",
            name: "muxed destination id 0",
            edit: |d, c: &Ctx| {
                payment_destination(d, muxed_account_with_id(&c.destination.verifying_key(), 0))
            },
        },
        Variant {
            field: "op.payment.destination.mux_id",
            name: "muxed destination id 2^64-1",
            edit: |d, c: &Ctx| {
                payment_destination(
                    d,
                    muxed_account_with_id(&c.destination.verifying_key(), u64::MAX),
                )
            },
        },
        Variant {
            field: "tx.source_account.mux_id",
            name: "muxed source id 2^63",
            edit: |d, c: &Ctx| {
                d.tx.source_account = MuxedAccount::MuxedEd25519(MuxedAccountMed25519 {
                    id: 1 << 63,
                    ed25519: Uint256(c.source.verifying_key().to_bytes()),
                })
            },
        },
        Variant {
            field: "tx.seq_num",
            name: "sequence number one too high",
            edit: |d, _| d.tx.seq_num = SequenceNumber(d.tx.seq_num.0 + 1),
        },
        Variant {
            field: "tx.fee",
            name: "fee 99 stroops",
            edit: |d, _| d.tx.fee = 99,
        },
        Variant {
            field: "tx.cond.time_bounds",
            name: "max time bound in the past",
            edit: |d, _| {
                d.tx.cond = Preconditions::Time(TimeBounds {
                    min_time: TimePoint(0),
                    max_time: TimePoint(1),
                })
            },
        },
        Variant {
            field: "tx.signatures",
            name: "signed by another key",
            edit: |d, c: &Ctx| d.signers = vec![c.stranger.clone()],
        },
    ]
}
