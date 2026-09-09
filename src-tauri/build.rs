fn main() {
    println!("cargo:rerun-if-env-changed=ZNET_PRODUCT");
    println!("cargo:rerun-if-changed=../products/desktop.json");
    let features = [
        ("tool-dns", "CARGO_FEATURE_TOOL_DNS"),
        ("tool-route", "CARGO_FEATURE_TOOL_ROUTE"),
        ("tool-node-probe", "CARGO_FEATURE_TOOL_NODE_PROBE"),
    ];
    let selected: Vec<&str> = features
        .iter()
        .filter(|(_, env)| std::env::var_os(env).is_some())
        .map(|(name, _)| *name)
        .collect();
    if let Ok(product) = std::env::var("ZNET_PRODUCT") {
        let products: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string("../products/desktop.json").expect("product catalog"),
        )
        .expect("valid product catalog");
        let expected: Vec<&str> = products[&product]
            .as_array()
            .expect("unknown ZNET_PRODUCT")
            .iter()
            .map(|value| value.as_str().expect("feature name"))
            .collect();
        assert_eq!(
            selected, expected,
            "frontend/backend product selection mismatch; use pnpm product"
        );
    }
    // Release binaries embed frontend assets. Reject stale or differently selected assets.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        println!("cargo:rerun-if-changed=../build/product-composition.json");
        let assets: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string("../build/product-composition.json")
                .expect("build frontend product before release binary"),
        )
        .expect("valid frontend product manifest");
        let expected: Vec<&str> = assets["features"]
            .as_array()
            .expect("frontend feature list")
            .iter()
            .map(|value| value.as_str().expect("feature name"))
            .collect();
        assert_eq!(
            selected, expected,
            "embedded frontend/backend features differ"
        );
    }
    tauri_build::build()
}
