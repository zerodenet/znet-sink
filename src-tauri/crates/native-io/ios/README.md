# iOS native document IO

The unsigned Swift package exports `init_plugin_znet_native_io` for the Rust
Tauri mobile plugin and uses the same generated `../.tauri/tauri-api` package
location as Tauri's installed dialog plugin. The package deployment baseline is
iOS 15. No signing account or signing material is created by this adapter.

`pickRead`, `save`, and `share` accept the Rust-prepared camelCase wire request.
The native policy independently validates action/purpose, URL-safe operation ID,
purpose budget, safe ASCII leaf filename, MIME, and ordinary-export credential
acknowledgment. Configuration/settings exports require JSON; redacted diagnostics
also allow plain text and ZIP. Host preparation remains responsible for ordinary
configuration ownership, settings exclusions, and actual diagnostic redaction.
Unknown/protected purposes cannot be exported through this bridge.

Document picking uses the single URL supplied by the OS delegate and a transient
security scope with coordinated, bounded stream reads. No path, URL, bookmark or
permanent document grant crosses into the frontend. Native private stage lease
cleanup is attempted after completion and from the adapter's own root on next startup.
Staged files receive complete data protection before presentation.

Save exports a copy of the staged file through `UIDocumentPickerViewController`.
After the OS callback, the adapter attempts a bounded coordinated read and checks
the byte count and SHA-256 digest against the staged content. An inaccessible or
different result is `result-unknown`, never a successful save. Share maps completed
system activity to `share-accepted`; this does not confirm remote delivery.

Every interaction has a separate native object identity. Busy requests are
rejected, and late delegate/IO callbacks cannot complete a new interaction even
when caller operation IDs are reused. Backgrounding alone preserves the operation
because Files/share may legitimately continue elsewhere. Foreground reconciliation
reports an unknown result if UIKit lost a previously presented interaction without
a terminal callback. The adapter does not use UI JavaScript timers or clipboard
fallbacks.

On Linux with Swift 5.7+ run:

```sh
swift test --package-path src-tauri/crates/native-io/ios
```

Only the Foundation policy/IO target is built there.
On an Apple host, the generated Tauri package and iOS SDK are required for the UIKit
target. The tests cover purpose/authorization/size boundaries, traversal rejection,
strict base64, bounded reads, lease isolation/cleanup, and response field limits.
UIKit SDK compilation, device Files providers, share callbacks, background/process
recovery and memory pressure still require platform validation. No iPhone VPN
acceptance or signed build is implied by these sources or Foundation tests.

Cloud verification on 2026-10-13: Swift 6.1.3 on Linux compiled and passed all eight
XCTest cases. The UIKit source passed `swiftc -frontend -parse`; this is a syntax
check and does not typecheck UIKit, CryptoKit, or the Tauri SDK against an iOS SDK.

API references: [Apple export picker](https://developer.apple.com/documentation/uikit/uidocumentpickerviewcontroller/init(forexporting:ascopy:)),
[Apple opening picker](https://developer.apple.com/documentation/uikit/uidocumentpickerviewcontroller/init(foropeningcontenttypes:ascopy:)).

## License provenance inventory

`scripts/license-swift.mjs` inventories this unsigned Swift library independently
from a generated iOS application. Supply locked Cargo metadata, the actual Apple
build scratch directory, and its generated `Package.resolved` if it is outside that
directory:

```sh
node scripts/license-swift.mjs \
  --cargo-metadata /tmp/cargo-metadata.json \
  --swift-build-path /tmp/native-io-ios-build \
  --target aarch64-apple-ios-sim \
  --build-log /tmp/native-io-xcodebuild.log \
  --output /tmp/native-io-swift-licenses.json
```

Optional `--resolved-file`, `--sdk-path`, `--sdk-data`, and `--artifact` arguments
provide the actual build lock, SDK framework metadata, SDKSettings JSON, and static
archive. Tauri's original Swift sources and license texts are found through its
exact Cargo metadata dependency, without reading generated `.tauri` source copies.
Remote Swift package texts require the actual Swift pin and a SwiftPM checkout
whose Git HEAD matches that revision. SwiftRs's Cargo version or another plugin's
lockfile cannot substitute for the actual Swift resolution.

The inventory contains exact observed license/notice texts and their hashes. It
marks source declarations, resolved packages, and successful target compilation
separately. Apple framework imports are platform-provided dependencies, with no
invented SPDX license or redistribution notice. A supplied archive must have an
observed Mach-O object of the requested CPU and iOS device/simulator platform.
Foundation tests on Linux remain separate from UIKit library compilation. Missing
Swift locks, source texts, SDK information, compilation evidence, or artifacts
remain explicit gaps. This library inventory does not prove what a future iOS
application bundles, signs, or runs on a device.

`node scripts/test-license-swift.mjs` runs independent temporary fixtures for pin
identity, exact text evidence, stale checkouts, archive rejection, deterministic
output, and actual build-log architecture selection. `license-swift-targets.mjs`
reads successful Simulator compiler logs; multiple observed architectures produce
separate target inventories in CI, without assuming the runner's architecture.
