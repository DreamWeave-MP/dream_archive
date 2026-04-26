use std::path::PathBuf;

#[must_use]
pub fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[must_use]
pub fn ba2_fixture(path: &str) -> PathBuf {
    fixture("tests/fixtures/ba2").join(path)
}
