//! The xdr types the fuzzer drives, each decoded and re-encoded in process by stellar-xdr the
//! way core calls it (`T::from_xdr` with the corpus decode limits) and generated from raw
//! bytes through the crate's `arbitrary` derives.

use arbitrary::{Arbitrary, Unstructured};
use stellar_xdr as x;
use stellar_xdr::{Limits, ReadXdr, WriteXdr};

macro_rules! fuzz_types {
    ($($name:ident),* $(,)?) => {
        /// Every type both testees decode: the go testee's type table is generated from the
        /// corpus manifest, so a type outside it would only ever come back unsupported.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum FuzzType {
            $($name),*
        }

        impl FuzzType {
            pub const ALL: &'static [FuzzType] = &[$(FuzzType::$name),*];

            pub fn name(self) -> &'static str {
                match self {
                    $(FuzzType::$name => stringify!($name)),*
                }
            }

            pub fn from_name(name: &str) -> Option<FuzzType> {
                match name {
                    $(stringify!($name) => Some(FuzzType::$name),)*
                    _ => None,
                }
            }

            /// Decodes the whole input and encodes the value again under the same limits.
            pub fn roundtrip(self, bytes: &[u8], limits: Limits) -> Result<Vec<u8>, stellar_xdr::Error> {
                match self {
                    $(FuzzType::$name => x::$name::from_xdr(bytes, limits.clone())?.to_xdr(limits)),*
                }
            }

            /// One value built from `u`, encoded without limits: the decode limits are what is
            /// under test, so generation must not apply them first.
            pub fn arbitrary(self, u: &mut Unstructured<'_>) -> Option<Vec<u8>> {
                match self {
                    $(FuzzType::$name => x::$name::arbitrary(u).ok()?.to_xdr(Limits::none()).ok()),*
                }
            }
        }
    };
}

fuzz_types!(
    TransactionEnvelope,
    LedgerCloseMeta,
    TransactionResultMeta,
    TransactionResultMetaV1,
    LedgerEntry,
    ContractEvent,
    ScVal,
    SorobanAuthorizationEntry,
    ScSpecFunctionV0,
    Asset,
    SignerKey,
);

impl FuzzType {
    /// The decode depth core and the corpus apply to a value of this type.
    pub fn depth(self) -> u32 {
        shapekit_manifest::decode_depth(self.name())
    }

    /// The corpus decode limits for an input of `len` bytes: the length budget is the input
    /// itself on both sides, so neither decoder can be talked into allocating more.
    pub fn limits(self, len: usize) -> Limits {
        shapekit_manifest::decode_limits(self.name(), len)
    }

    /// Whether core's verdict on bytes of this type is rs-stellar-xdr's at the corpus depth.
    /// From protocol 28 core refuses any envelope its rust parser cannot read
    /// (`can_parse_transaction`); every other type here is something core produces, so a
    /// decoder that refuses or accepts other bytes of it is not overruled by core.
    pub fn core_decodes(self) -> bool {
        self == FuzzType::TransactionEnvelope
    }
}

/// Values nested in a decoded corpus entry that are themselves fuzz types, so the small types
/// get real seeds (contract arguments, auth trees, contract data) and not only generated ones.
pub fn nested_seeds(ty: FuzzType, bytes: &[u8]) -> Vec<(FuzzType, Vec<u8>)> {
    let mut out = Vec::new();
    let limits = ty.limits(bytes.len());
    match ty {
        FuzzType::TransactionEnvelope => {
            if let Ok(env) = x::TransactionEnvelope::from_xdr(bytes, limits) {
                for op in operations(&env) {
                    if let x::OperationBody::InvokeHostFunction(ihf) = &op.body {
                        if let x::HostFunction::InvokeContract(args) = &ihf.host_function {
                            for a in args.args.iter() {
                                push(&mut out, FuzzType::ScVal, a);
                            }
                        }
                        for auth in ihf.auth.iter() {
                            push(&mut out, FuzzType::SorobanAuthorizationEntry, auth);
                        }
                    }
                }
            }
        }
        FuzzType::LedgerEntry => {
            if let Ok(entry) = x::LedgerEntry::from_xdr(bytes, limits)
                && let x::LedgerEntryData::ContractData(cd) = &entry.data
            {
                push(&mut out, FuzzType::ScVal, &cd.key);
                push(&mut out, FuzzType::ScVal, &cd.val);
            }
        }
        _ => {}
    }
    out
}

fn push<T: WriteXdr>(out: &mut Vec<(FuzzType, Vec<u8>)>, ty: FuzzType, v: &T) {
    if let Ok(b) = v.to_xdr(Limits::none()) {
        out.push((ty, b));
    }
}

fn operations(env: &x::TransactionEnvelope) -> Vec<&x::Operation> {
    match env {
        x::TransactionEnvelope::TxV0(e) => e.tx.operations.iter().collect(),
        x::TransactionEnvelope::Tx(e) => e.tx.operations.iter().collect(),
        x::TransactionEnvelope::TxFeeBump(e) => match &e.tx.inner_tx {
            x::FeeBumpTransactionInnerTx::Tx(inner) => inner.tx.operations.iter().collect(),
        },
    }
}
