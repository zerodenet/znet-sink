# Android application host

This is the generated Tauri Android application, retained as reviewable source.
It includes the actual native document plugin through Cargo/Tauri mobile plugin
metadata. Generated plugin bindings, private machine paths and build products
remain ignored. Android minimum SDK is 24; the arm64 build covers the Galaxy S8
CPU architecture, while Galaxy S8 operation still requires device acceptance.

The activity applies native system-bar/cutout/keyboard insets and dispatches
cancelable `mobile:back` to the host shell before exiting a root page. A visible
keyboard is hidden before navigating. Closing the UI does not issue a VPN stop
command or own any future native runtime service. Device
behavior, including Samsung keyboard/gesture combinations, remains unvalidated.

Release and Debug signing configurations are explicitly unset. Building this
host must not create signing credentials or a debug keystore. The unsigned
Release artifact is not directly installable and is not a signed distribution.
There is no VpnService in this host: the mobile runtime capability snapshot
continues to explain why VPN forwarding is unavailable. A successful APK build
cannot satisfy the pending core/platform data-plane acceptance.

Use a JDK (17 or 21), SDK platform 36, build tools 35.0.0, NDK 27.3.13750724,
aarch64-linux-android Rust target and the repository's locked pnpm dependencies:

```sh
pnpm tauri android build --apk --target aarch64 --ci
python3 scripts/verify-unsigned-android-apk.py \
  src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk
```

The CLI builds frontend assets and the actual Rust mobile entry point, supplies
its plugin projects, then invokes the Gradle Release variant. Avoid re-running
`android init` over reviewed host changes: it regenerates project files. The
Gradle wrapper has the official 8.14.3 distribution SHA-256 pinned. Private
profiles are excluded from Android backup. The generic file-opening provider
is confined to native document staging; the document plugin's own provider
adds byte budgets, read-only grants, expiry checks and native cleanup.

`android-unsigned-host.yml` rebuilds this actual host and verifies package
contents, minimum SDK and absence of v1/v2/v3 signing entries. It neither
installs the APK nor creates signing credentials, runs a device/emulator, or
claims VPN/data-plane validation.

The frontend currently targets Chromium 111 or newer. API 24 alone does not
prove WebView compatibility. Before Wry loads the application URL, the host
checks the active WebView UA; old or unidentified engines have JavaScript
disabled and show a native, scrollable explanation. It offers the actual
WebViewCompat-discovered provider's application details, an explicit recheck and
UI close. It does not guess a package name or grant permissions. Provider
updates may require the OS to restart the app. Original Galaxy S8 firmware and
actual provider-update behavior remain unvalidated.
