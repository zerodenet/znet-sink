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
    // Use the target, not the build-script host, so cross-compilation works too.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Tauri's resource manifest does not reach the library unit-test runner.
        // Embed it through the linker for every executable, including tests, to
        // prevent STATUS_ENTRYPOINT_NOT_FOUND when loading Common-Controls APIs.
        // https://github.com/tauri-apps/tauri/pull/4383#issuecomment-1212221864
        // Disable only the resource manifest to avoid duplicates; Tauri still
        // generates the app's icon and version resources.
        tauri_build::try_build(
            tauri_build::Attributes::new()
                .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
        )
        .expect("failed to build Tauri resources");

        let manifest = std::path::PathBuf::from(
            std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"),
        )
        .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        // Keep Tauri's default manifest behavior without adding a UAC fragment.
        println!("cargo:rustc-link-arg=/MANIFESTUAC:NO");
    } else {
        tauri_build::build()
    }
}
