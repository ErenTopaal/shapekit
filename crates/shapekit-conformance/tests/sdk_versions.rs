//! The committed `sdk-version` fragments name exactly the versions conformance/versions.toml
//! lists, so adding or dropping a version without rerunning the all-versions run fails here.

use std::path::PathBuf;

use shapekit_conformance::sdk_versions;
use shapekit_conformance::versions::VersionsFile;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn fragments_follow_the_versions_file() {
    let file = VersionsFile::load(&root().join("conformance/versions.toml")).unwrap();
    for sdk in file.sdk.keys() {
        sdk_versions::check(&root(), &file, sdk).unwrap();
    }
}
