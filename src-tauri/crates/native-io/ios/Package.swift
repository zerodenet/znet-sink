// swift-tools-version:5.7
import PackageDescription

// The policy and bounded IO tests also run without UIKit on Linux.
#if os(Linux)
let nativeProducts: [Product] = []
let nativeDependencies: [Package.Dependency] = []
let nativeTargets: [Target] = []
#else
let nativeProducts: [Product] = [
    // Tauri's SwiftLinker links the static product by the owning Cargo package
    // name. Keep the module name stable while matching that actual link input.
    .library(name: "tauri-plugin-znet-native-io", type: .static, targets: ["znet-native-io"])
]
let nativeDependencies: [Package.Dependency] = [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
]
let nativeTargets: [Target] = [
    .target(name: "znet-native-io", dependencies: ["NativeIoPolicy", .byName(name: "Tauri")],
            path: "Sources/NativeIoPlugin")
]
#endif

let package = Package(
    name: "tauri-plugin-znet-native-io",
    platforms: [.iOS(.v15)],
    products: nativeProducts,
    dependencies: nativeDependencies,
    targets: [
        .target(name: "NativeIoPolicy", path: "Sources/NativeIoPolicy"),
        .testTarget(name: "NativeIoPolicyTests", dependencies: ["NativeIoPolicy"])
    ] + nativeTargets
)
