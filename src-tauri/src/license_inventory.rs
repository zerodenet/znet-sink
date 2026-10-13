//! Public dependency notices generated for this exact Rust target/profile.
//! No kernel query or runtime permission is required to inspect license materials.
pub fn embedded_snapshot() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/license-inventory.rust.json"
    )))
    .expect("build-validated license inventory")
}
