//! Release order of sdk versions, shared by the findings record and the run report so both
//! name the same version when they say how to rerun an entry.

use std::cmp::Ordering;

/// Orders two versions as releases are ordered (semver precedence): numeric parts by value
/// (`v0.10.0` after `v0.9.0`), a pre-release before its release, build metadata (`+...`)
/// ignored, and a leading `v` ignored.
pub fn version_order(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (Vec<&str>, Option<&str>) {
        let v = v.strip_prefix('v').unwrap_or(v);
        // build metadata has no precedence, so it never takes part in the order
        let v = v.split_once('+').map_or(v, |(v, _)| v);
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (v, None),
        };
        (core.split('.').collect(), pre)
    }
    fn part(a: &str, b: &str) -> Ordering {
        match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            // a numeric part sorts before a named one, as semver orders identifiers
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => a.cmp(b),
        }
    }
    fn parts(xs: &[&str], ys: &[&str]) -> Ordering {
        xs.iter()
            .zip(ys)
            .map(|(p, q)| part(p, q))
            .find(|o| o.is_ne())
            .unwrap_or_else(|| xs.len().cmp(&ys.len()))
    }
    let ((ca, pa), (cb, pb)) = (split(a), split(b));
    parts(&ca, &cb).then_with(|| match (pa, pb) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => {
            let (xs, ys): (Vec<&str>, Vec<&str>) = (x.split('.').collect(), y.split('.').collect());
            parts(&xs, &ys)
        }
    })
}

/// The newest of `versions` in release order. Versions of equal precedence (`1.0.0+a` and
/// `1.0.0+b`) are told apart by their text, so the choice does not depend on input order.
pub fn newest<'v>(versions: impl IntoIterator<Item = &'v str>) -> Option<&'v str> {
    versions
        .into_iter()
        .max_by(|a, b| version_order(a, b).then_with(|| a.cmp(b)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::*;

    #[test]
    fn versions_order_as_releases() {
        assert_eq!(version_order("v0.10.0", "v0.9.0"), Greater);
        assert_eq!(version_order("0.9.0", "v0.9.0"), Equal);
        assert_eq!(version_order("1.0.0-rc.1", "1.0.0"), Less);
        assert_eq!(version_order("1.0.0-rc.10", "1.0.0-rc.9"), Greater);
        assert_eq!(version_order("17.4.0", "17.10.0"), Less);
        assert_eq!(version_order("1.2", "1.2.1"), Less);
    }

    #[test]
    fn build_metadata_has_no_precedence() {
        assert_eq!(version_order("1.0.9+build.5", "1.0.10"), Less);
        assert_eq!(version_order("1.0.0+a", "1.0.0+b"), Equal);
        assert_eq!(version_order("1.0.0+zzz", "1.0.0"), Equal);
        assert_eq!(version_order("1.0.0-rc.1+b7", "1.0.0"), Less);
        assert_eq!(version_order("1.0.0-rc.2+b1", "1.0.0-rc.10"), Less);
    }

    #[test]
    fn newest_is_in_release_order() {
        assert_eq!(newest(["v0.9.0", "v0.10.0", "v0.2.0"]), Some("v0.10.0"));
        assert_eq!(newest(["2.0.0-rc.1", "1.9.0", "2.0.0"]), Some("2.0.0"));
        assert_eq!(newest(["1.0.0+b", "1.0.0+a"]), newest(["1.0.0+a", "1.0.0+b"]));
        assert_eq!(newest([]), None);
    }
}
