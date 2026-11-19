//! Session log of asset imports and reimports.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportActivityEntry {
    pub at_ms: u64,
    pub file: String,
    pub importer: String,
    pub result: String,
    pub duration_ms: u64,
    /// Output hash of the imported bytes and settings. Empty when the import failed.
    pub output_hash: String,
}
