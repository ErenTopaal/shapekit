//! The repository's own test contracts (`contracts/wasm/`), built by
//! `contracts/build.sh` with soroban-sdk 29.0.1 and checked against
//! `contracts/wasm/SHA256SUMS` by the tests, plus older-sdk builds of echo
//! from `contracts/build-legacy.sh` (`contracts/wasm/p<N>/`).

/// `echo`: stores, emits and returns a value, and writes temporary entries
/// with `put_temp`, which is what the settings bootstrap uses to store a
/// config upgrade set.
pub const ECHO_WASM: &[u8] = include_bytes!("../../../contracts/wasm/echo.wasm");

/// Interface version the contracts were built for: soroban-sdk 29 writes
/// protocol 29 into `contractenvmetav0`, and soroban-env-host refuses such a
/// wasm on a ledger running an earlier protocol ("contract protocol number
/// is newer than host", `check_contract_interface_version`, 30.0.0).
pub const CONTRACTS_INTERFACE_PROTOCOL: u32 = 29;

/// `echo` built from the same source by `contracts/build-legacy.sh` against
/// older soroban-sdk releases, for chains below protocol 29. The suffix is
/// the interface protocol the build declares: soroban-sdk 22.0.11 (22),
/// 23.5.3 (23, used for protocol 24 since no release declares 24) and
/// 28.0.0 (28).
pub const ECHO_WASM_P22: &[u8] = include_bytes!("../../../contracts/wasm/p22/echo.wasm");
pub const ECHO_WASM_P23: &[u8] = include_bytes!("../../../contracts/wasm/p23/echo.wasm");
pub const ECHO_WASM_P28: &[u8] = include_bytes!("../../../contracts/wasm/p28/echo.wasm");

/// The newest committed echo build a ledger at `protocol` can run: the one
/// with the highest interface protocol not above it.
pub fn echo_wasm_for(protocol: u32) -> Option<&'static [u8]> {
    [
        (CONTRACTS_INTERFACE_PROTOCOL, ECHO_WASM),
        (28, ECHO_WASM_P28),
        (23, ECHO_WASM_P23),
        (22, ECHO_WASM_P22),
    ]
    .into_iter()
    .find(|(p, _)| *p <= protocol)
    .map(|(_, w)| w)
}

/// The protocol in a wasm's `contractenvmetav0` section, if it has one.
pub fn interface_protocol(wasm: &[u8]) -> Option<u32> {
    let mut i = 8;
    if wasm.get(..4) != Some(b"\0asm".as_slice()) {
        return None;
    }
    while i < wasm.len() {
        let id = wasm[i];
        i += 1;
        let (len, next) = leb128(wasm, i)?;
        let end = next.checked_add(len)?;
        let body = wasm.get(next..end)?;
        if id == 0 {
            let (name_len, at) = leb128(body, 0)?;
            let name = body.get(at..at + name_len)?;
            if name == b"contractenvmetav0" {
                // ScEnvMetaEntry: kind (0, interface version), protocol, pre-release
                let data = body.get(at + name_len..)?;
                let kind = u32::from_be_bytes(data.get(..4)?.try_into().ok()?);
                if kind != 0 {
                    return None;
                }
                return Some(u32::from_be_bytes(data.get(4..8)?.try_into().ok()?));
            }
        }
        i = end;
    }
    None
}

fn leb128(b: &[u8], mut i: usize) -> Option<(usize, usize)> {
    let mut v: usize = 0;
    let mut shift = 0;
    loop {
        let byte = *b.get(i)?;
        i += 1;
        v |= usize::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((v, i));
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn echo_is_the_committed_build() {
        let sums = include_str!("../../../contracts/wasm/SHA256SUMS");
        let want = sums
            .lines()
            .find_map(|l| l.strip_suffix("  echo.wasm"))
            .expect("echo.wasm in SHA256SUMS");
        assert_eq!(hex::encode(Sha256::digest(ECHO_WASM)), want);
        assert_eq!(
            interface_protocol(ECHO_WASM),
            Some(CONTRACTS_INTERFACE_PROTOCOL)
        );
    }

    #[test]
    fn legacy_echo_builds_are_committed_and_declare_their_protocol() {
        let builds = [
            (
                ECHO_WASM_P22,
                22,
                include_str!("../../../contracts/wasm/p22/SHA256SUMS"),
            ),
            (
                ECHO_WASM_P23,
                23,
                include_str!("../../../contracts/wasm/p23/SHA256SUMS"),
            ),
            (
                ECHO_WASM_P28,
                28,
                include_str!("../../../contracts/wasm/p28/SHA256SUMS"),
            ),
        ];
        for (wasm, protocol, sums) in builds {
            let want = sums
                .lines()
                .find_map(|l| l.strip_suffix("  echo.wasm"))
                .expect("echo.wasm in SHA256SUMS");
            assert_eq!(hex::encode(Sha256::digest(wasm)), want, "p{protocol}");
            assert_eq!(interface_protocol(wasm), Some(protocol));
        }
    }

    #[test]
    fn echo_for_a_protocol_never_declares_a_newer_one() {
        assert_eq!(echo_wasm_for(21), None);
        for protocol in 22..=30 {
            let wasm = echo_wasm_for(protocol).expect("a build");
            let declared = interface_protocol(wasm).expect("declared");
            assert!(declared <= protocol, "{protocol}: {declared}");
        }
        assert_eq!(echo_wasm_for(24), Some(ECHO_WASM_P23));
        assert_eq!(echo_wasm_for(28), Some(ECHO_WASM_P28));
        assert_eq!(echo_wasm_for(30), Some(ECHO_WASM));
    }
}
