# Traffic sample recovery

## Symptom

The overview intentionally hides live rates after ten seconds without a sample. A running kernel or healthy TUN is not evidence of fresh traffic observations. Before this change, active flows had periodic reconciliation but traffic depended only on stats.sampled pushes.

## Change

The GUI event subscription now owns a traffic watchdog. After three seconds without a fresh push, it queries runtime/stats/runtime over its existing pinned connection. Each read is capped at 500 ms; pending IPC operations defer recovery. Successful queries continue at a bounded rate while pushes are missing. Fresh pushes stop those extra reads. Failure does not fabricate zero counters, retire a busy transport, or restart the kernel.

A changed runtime/configuration during the read rejects the result; a different runtime from the subscription causes resynchronization. Incomplete counters are rejected. Delayed, duplicate, and too-close samples cannot replace a newer rate. A gap longer than ten seconds resets the rate baseline instead of presenting an average over the interruption as current speed.

Recovery samples use the existing Rust traffic bridge for the overview, tray and traffic ball, keeping their rate calculation shared.

## Installed-client regression 2026-09-29

The installed build 426 was retrying an obsolete observation endpoint after a kernel installation changed the executable from a desktop test binary to the managed core directory. On Unix the private socket lives beside that executable, so ordinary queries used the new address while the event forwarder retained the old one. The recovery watchdog belongs to the forwarder and could not run before that old connection succeeded.

Kernel installation and rollback completion now rebuild the observation subscription from current settings. Committed settings changes that alter the resolved endpoint notify the same lifecycle owner. Stop/start is serialized, late notifications cannot revive a disposed stream, and rate events from an older generation are rejected. Normal network changes keep the subscription intact.

Read-only process sampling also found the IPC reader blocked in synchronous diagnostic log rotation, with received stats events about twenty seconds behind their kernel timestamps. Disk writes, rotation, connection-history persistence and log projection now run on one bounded diagnostic worker. The reader creates a byte-bounded diagnostic projection before enqueueing and never waits for storage; there is no duplicate in-memory frame ring. Queue overflow drops diagnostics rather than traffic events, reports the skipped count from the worker, and records lost completed-flow history in the existing failure counter. Abrupt application termination may lose queued diagnostic frames.

Validation: 12 frontend observation lifecycle tests passed; frontend type checking and production build passed; Rust workspace tests reported 758 passed, zero failed and seven ignored. The new two-peer IPC rebind test and an explicit contract check against the installed Zero executable both passed. The latter launches only a private no-inbound child; the running kernel, TUN, system proxy and installed application were not restarted or replaced during validation. Restored rates in the user's installed application still require installation of the corrected client.

## Files

- src-tauri/src/services/gui_events.rs
- src-tauri/src/services/traffic_sampler.rs
- src-tauri/src/services/traffic_sampler/recovery.rs
- src-tauri/src/services/traffic_sampler/recovery_tests.rs
- src-tauri/src/kernel/observation.rs
- src-tauri/crates/engine-client/src/observation.rs
- src-tauri/crates/engine-client/tests/observation.rs

## Verification 2026-09-28

- A direct, read-only subscription to the currently running local kernel received five stats.sampled events in four seconds. This does not establish what happened at the exact screenshot time.
- Ten engine-client observation tests, four traffic sampler/recovery tests and two GUI event tests passed.
- Ten frontend observation tests, event lifecycle checks and 23 overview logic tests passed.
- Rust formatting and whitespace checks passed.
- No client installed, kernel restarted, or release published. Intermittent loss and recovery in the installed client remain pending acceptance.


## IPC diagnostic resource limits

- Default capture stores summaries (a 2 KiB projection budget); detail capture is explicitly enabled under Debug > IPC and expires after five minutes. Detail projections have a 16 KiB budget. Configuration/credential fields are redacted in both modes. Oversized trees, arrays, strings and deep values are marked as truncated; these are not exact wire transcripts.
- Completed connection records remain captured independently of detail mode, with a 32 KiB projection budget. This path serves connection history, never reconstruction of traffic totals. The existing history retention and loss reporting remain separate.
- The queue holds at most 128 bounded frames. There is no second in-memory ring. The disk store keeps one active 1 MiB segment and four 1 MiB archives; append-time rotation only renames/removes files. Existing oversized files are reduced once to a bounded tail on startup. Records above 64 KiB are rejected; query pages are bounded to 512 KiB of input records.
- Storage totals, clear operations and diagnostic exports include all IPC segments. Ordinary running logs retain only control/probe summaries and do not mirror background policy snapshots or entire IPC JSON payloads.
- OS proxy observation shares a five-second snapshot (one second for errors) and coalesces concurrent readers on a blocking worker. Mutating guard decisions use fresh OS reads; successful or partially failed mutations invalidate the cache. Original user proxy backups continue to be captured fresh.

These limits bound diagnostic retention and copying. They do not establish a post-installation RSS/CPU figure or prove that all allocator growth originates in diagnostics. Validate a newly installed build against the same sustained workload separately.
