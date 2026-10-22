# Android native IO adapter

`NativeIoPlugin` is a local Tauri plugin used through Rust's `PluginHandle`.
The host must not add a JavaScript invoke handler for it, and must serialize
optional fields as absent rather than `null`. Imports default to JSON MIME.
The Rust host prepares ordinary configurations, redacted diagnostics and
settings through separate typed paths; this adapter does not accept protected
configuration or plugin material purposes. An acknowledgement is mandatory
before exporting an ordinary configuration that may include proxy credentials.

The Android library uses Tauri 2.11.2's `@Command`, `@ActivityCallback`, `Invoke`
and `Plugin` interfaces. It needs the generated `:tauri-android` project,
Android SDK 36, an Android Gradle plugin and Kotlin Android plugin provided by
the generated application. Its minimum SDK is 24. No storage or notification
permission, signing identity, account or persistent URI grant is requested.

`pickRead` uses `ACTION_OPEN_DOCUMENT` and reads at most the requested budget
plus one byte. Oversize imports never return partial data. `save` uses
`ACTION_CREATE_DOCUMENT`, writes, flushes and closes the stream, then compares a
bounded reread with the prepared bytes. A missing reread permission, IO failure
or mismatch reports `result-unknown`. Provider names, paths, URIs, exception
text and document contents never enter error responses or logs.

`share` uses an unexported narrow FileProvider and an explicit unexported
chooser callback receiver. A callback with `EXTRA_CHOSEN_COMPONENT` means
`share-accepted`, which confirms the chooser selection only. A missing callback
reports `result-unknown`: Android/OEM `ACTION_SEND` activity result codes cannot
reliably prove user cancellation or recipient delivery. File and read grants
remain for up to ten minutes. Native scheduling revokes and deletes expired
staging; cold start and foreground entry also clean up. If the process was
killed, deletion waits for the next native process start; the provider rejects
expired content on open/query even before cleanup. Staging is bounded to 32
files and 64 MiB. Nothing uses a WebView timer or clipboard.

Only one native operation may own a picker at a time. Tauri's Android
PluginManager has one activity-result callback slot shared by all plugins;
the host must also serialize other dialog activity requests with this adapter.
Recreated activities return `result-unknown`; the UI must keep its draft and
offer an explicit new operation rather than automatically replaying a write.

Run `:native-io:testDebugUnitTest` and `:native-io:compileDebugKotlin` from a
generated Android host that includes this project. JVM tests cover purpose,
action, filename, acknowledgement, MIME, encoded/decoded byte budgets, bounded
stream overflow and streams that temporarily return zero bytes. System picker
cancellation, cloud-provider reread permission, API 24 chooser callbacks,
recreation and receiver grant expiration still require emulator/device tests.

Platform references:
[Android document access](https://developer.android.com/training/data-storage/shared/documents-files)
and [Android Sharesheet](https://developer.android.com/training/sharing/send).
