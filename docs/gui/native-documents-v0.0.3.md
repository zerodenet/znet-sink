# Native document boundary for v0.0.3

The local `tauri-plugin-znet-native-io` adapter gives the host actual Android
SAF/ContentResolver/FileProvider and iOS Files/activity-controller operations.
This work is independent of the unimplemented mobile VPN/core runtime bridge.
The plugin exposes no WebView invoke handler: the host supplies the operation
identity and prepares the permitted payload. UI commands cannot submit arbitrary
export bytes, native paths or document URIs.

## Purpose, ownership and receipts

Configuration/settings imports use different 16 MiB/2 MiB budgets. Ordinary
configuration exports require an explicit credential warning acknowledgement
and ordinary-profile ownership. Settings exports use a host serializer that
excludes secrets and grants; diagnostic exports use a fixed public metadata whitelist and a
10 MiB budget. Protected plugin material is rejected before native interaction.
Purpose alone does not prove arbitrary JSON is secret-free: the host serializer
and redactor remain the authorities for those two payloads.

The current native diagnostic file contains build/platform contract facts and
the time, source and level of at most 200 local log records. Messages, fields,
debug payloads, configuration and subscription contents are excluded. The
preview states this reduced range explicitly; complete redacted log/capture
exports and device validation remain separate work.

Host request and native response share one immutable operation ID. Rust rejects
wrong IDs, wrong-action success, missing or oversized content, byte-count
mismatches and unsafe error text. Reads return complete bounded bytes; oversize
inputs never return a truncated configuration. The UI receives UTF-8 content
for validation, never the native path, URI, bookmark or staging location.

`read` confirms complete reading. `saved` requires successful bounded rereading
and content comparison. `share-accepted` confirms system chooser selection on
Android or successful activity completion on iOS; neither confirms recipient
storage or remote delivery. `cancelled`, `failed` and `result-unknown` remain
separate. Once an export is dispatched, bridge loss is ambiguous and must not
cause automatic replay. Picker cancellation preserves drafts and existing data.

## Native lifetime and grants

Android uses `ACTION_OPEN_DOCUMENT` and `ACTION_CREATE_DOCUMENT`, transient
ContentResolver grants and no broad storage permission. Native worker threads
perform bounded IO. The sharing FileProvider exposes only private staging,
checks the ten-minute lease when opening/querying, refuses write access, and
uses native cleanup/revocation with cold-start recovery. Staging is bounded to
32 files and 64 MiB. A killed process cannot serve an expired file when its
provider restarts. An already-open receiver FD cannot be retroactively recalled.

Android shares without a reliable chooser callback report unknown. OEM
`ACTION_SEND` activity-result cancellation does not reliably prove user intent.
The adapter preserves the shared Tauri activity-result slot until that activity
finishes, even when a chooser selection receipt arrives first. Host dialogs
must not launch concurrently through another plugin while a document operation
owns that slot.

iOS uses UIDocumentPickerViewController, balanced security-scoped access,
NSFileCoordinator, private file leases and UIActivityViewController. Exported
staging uses complete file protection. Save verification compares bytes and
SHA-256 before declaring success. Native object identity rejects old callbacks;
a foreground return with a lost presentation reports unknown. UIKit and native
queues own these operations, not WebView timers. Package iOS 15 is the current
adapter API floor, not a frozen product support decision.

## Evidence and remaining acceptance

Cloud Linux validation used the actual installed Tauri Android source and
tracked `scripts/test-native-io-android.sh`: Gradle 8.14.3, AGP 8.9.3, Kotlin
1.9.25, JDK 21.0.12.1, compile SDK 36, build tools 35.0.0, minimum API 24.
`:native-io:testDebugUnitTest` and `:native-io:assembleDebug` completed with all
11 JUnit cases passing. The produced AAR is a plugin library, not an application
APK or a Galaxy S8 acceptance result. Java source-target deprecation, an unused
native callback argument and Gradle deprecation warnings remain nonfatal.

The actual shared Rust policy source passed five tests. Swift 6.1.3 compiled
Foundation policy/IO sources and passed all eight XCTest cases on Linux; UIKit
source syntax parsing also passed. That does not typecheck UIKit against an
Apple SDK. The new `native-documents.yml` CI performs Android library compilation,
Swift policy tests and an unsigned iOS Simulator SDK build. The latter's result
must be recorded from CI; no signature or paid account is required or created.

Still unrun: system-provider cancellation/failure/revoked grants, readback from
cloud providers, save disk exhaustion, chooser receiver/lease expiration under
process death, Android API 24/Samsung S8 UI behavior, and actual iOS Files/share
callbacks and app lifecycle. A08/A19/C05/C11 remain partial until those system
cases and the corresponding host business flows pass. No native VPN forwarding,
APK installation, iPhone VPN, signed distribution or store release is implied.

References: [Android document access](https://developer.android.com/training/data-storage/shared/documents-files),
[Android sharing](https://developer.android.com/training/sharing/send),
[Apple document picker](https://developer.apple.com/documentation/uikit/uidocumentpickerviewcontroller)
and [Apple activity controller](https://developer.apple.com/documentation/uikit/uiactivityviewcontroller).
