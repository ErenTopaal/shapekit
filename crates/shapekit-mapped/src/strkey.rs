//! Stored address bytes (a strkey version byte, then the payload, as wallet-backend's
//! `AddressBytea` keeps them) back to strkeys, strictly: the bytes are never padded or cut.

use stellar_strkey::Strkey;

/// The version byte of each strkey letter (SEP-23).
pub fn version_byte(letter: &str) -> Option<u8> {
    Some(match letter {
        "G" => 6 << 3,
        "S" => 18 << 3,
        "M" => 12 << 3,
        "T" => 19 << 3,
        "X" => 23 << 3,
        "P" => 15 << 3,
        "C" => 2 << 3,
        "L" => 11 << 3,
        "B" => 1 << 3,
        "W" => 22 << 3,
        _ => return None,
    })
}

pub fn letter(version: u8) -> Option<&'static str> {
    crate::mapping::LETTERS
        .into_iter()
        .find(|l| version_byte(l) == Some(version))
}

fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= u16::from(b) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn base32(data: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::with_capacity(data.len().div_ceil(5) * 8);
    let mut buf: u64 = 0;
    let mut bits = 0;
    for &b in data {
        buf = (buf << 8) | u64::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// The strkey text of a version byte and payload, whatever the payload's length: the same as
/// go-stellar-sdk's `strkey.Encode`, which does not check the payload either.
pub fn encode(version: u8, payload: &[u8]) -> String {
    let mut d = Vec::with_capacity(payload.len() + 3);
    d.push(version);
    d.extend_from_slice(payload);
    let crc = crc16_xmodem(&d);
    d.extend_from_slice(&crc.to_le_bytes());
    base32(&d)
}

/// Decodes stored bytes into a strkey whose letter is one of `allowed`. The error says why
/// the bytes do not decode, or why the key they hold does not belong in the column.
pub fn decode_stored(b: &[u8], allowed: &[String]) -> Result<String, String> {
    if b.len() < 2 {
        return Err(format!(
            "{} stored bytes cannot hold a version byte and a key",
            b.len()
        ));
    }
    let Some(name) = letter(b[0]) else {
        return Err(format!(
            "version byte 0x{:02x} is not a strkey version",
            b[0]
        ));
    };
    let payload = &b[1..];
    let enc = encode(b[0], payload);
    // decoding checks the payload shape: sep-23 lengths and the cap-40 signed payload
    // structure. Strkey refuses S keys by design, so a seed is checked on its own type
    let decoded = if name == "S" {
        stellar_strkey::ed25519::PrivateKey::from_string(&enc).map(|_| ())
    } else {
        Strkey::from_string(&enc).map(|_| ())
    };
    if let Err(e) = decoded {
        return Err(format!(
            "{name} key with a {}-byte payload does not decode: {e}",
            payload.len()
        ));
    }
    if !allowed.iter().any(|a| a == name) {
        return Err(format!(
            "a {name} key is not a canonical value for this column"
        ));
    }
    Ok(enc)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT: &str = "GCCLUSWMBODKXUESRSM35CX545JJAHX2CFYCCN63RSQY5RRHEJCR4DC2";
    const SIGNER: &str = "PDDLGS6XTXODPNKK7OP4FHZSNGVJG3OE3GKGMYN47OP3PWM3SSTPCAAAAAQLTTLAOIMJNKCNM54J2UYSG6QYOCGN2XAXHXGHLYGIRPJKRQHFSQH534";

    pub fn stored(s: &str) -> Vec<u8> {
        let k = Strkey::from_string(s).unwrap();
        let (v, payload): (u8, Vec<u8>) = match k {
            Strkey::PublicKeyEd25519(p) => (version_byte("G").unwrap(), p.0.to_vec()),
            Strkey::SignedPayloadEd25519(p) => {
                let mut v = p.ed25519().to_vec();
                v.extend_from_slice(&(p.payload().len() as u32).to_be_bytes());
                v.extend_from_slice(p.payload());
                v.resize(32 + 4 + p.payload().len().div_ceil(4) * 4, 0);
                (version_byte("P").unwrap(), v)
            }
            _ => unreachable!(),
        };
        let mut out = vec![v];
        out.extend(payload);
        out
    }

    fn l(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn decodes_as_stored_and_refuses_cut_or_misplaced_keys() {
        let acct = stored(ACCOUNT);
        let signer = stored(SIGNER);
        assert_eq!(encode(acct[0], &acct[1..]), ACCOUNT);
        assert_eq!(decode_stored(&acct, &l(&["G", "C"])).unwrap(), ACCOUNT);
        assert_eq!(
            decode_stored(&signer, &l(&["G", "T", "X", "P"])).unwrap(),
            SIGNER
        );
        // 79ea2acd's AddressBytea keeps the version byte and the first 32 payload bytes
        assert!(decode_stored(&signer[..33], &l(&["P"])).is_err());
        let mut muxed = acct.clone();
        muxed[0] = version_byte("M").unwrap();
        assert!(decode_stored(&muxed, &l(&["G", "C", "M"])).is_err());
        assert!(decode_stored(&signer, &l(&["G", "C"])).is_err());
        assert!(decode_stored(&[0xff, 1, 2], &l(&["G"])).is_err());
        assert!(decode_stored(&[0x30], &l(&["G"])).is_err());
    }
}
