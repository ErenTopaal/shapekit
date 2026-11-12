//! The repository's own test contracts (`contracts/wasm/`), built by
//! `contracts/build.sh` with soroban-sdk 29.0.1 and checked against
//! `contracts/wasm/SHA256SUMS` by the tests.

/// `echo`: stores, emits and returns a value, and writes temporary entries
/// with `put_temp`, which is what the settings bootstrap uses to store a
/// config upgrade set.
pub const ECHO_WASM: &[u8] = include_bytes!("../../../contracts/wasm/echo.wasm");

/// Interface version the contracts were built for: soroban-sdk 29 writes
/// protocol 29 into `contractenvmetav0`, and soroban-env-host refuses such a
/// wasm on a ledger running an earlier protocol ("contract protocol number
/// is newer than host", `check_contract_interface_version`, 30.0.0).
pub const CONTRACTS_INTERFACE_PROTOCOL: u32 = 29;

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
}
