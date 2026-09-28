# Traffic sample recovery

## Symptom

The overview intentionally hides live rates after ten seconds without a sample. A running kernel or healthy TUN is not evidence of fresh traffic observations. Before this change, active flows had periodic reconciliation but traffic depended only on stats.sampled pushes.

## Change

The GUI event subscription now owns a traffic watchdog. After three seconds without a fresh push, it queries runtime/stats/runtime over its existing pinned connection. Each read is capped at 500 ms; pending IPC operations defer recovery. Successful queries continue at a bounded rate while pushes are missing. Fresh pushes stop those extra reads. Failure does not fabricate zero counters, retire a busy transport, or restart the kernel.

A changed runtime/configuration during the read rejects the result; a different runtime from the subscription causes resynchronization. Incomplete counters are rejected. Delayed, duplicate, and too-close samples cannot replace a newer rate. A gap longer than ten seconds resets the rate baseline instead of presenting an average over the interruption as current speed.

Recovery samples use the existing Rust traffic bridge for the overview, tray and traffic ball, keeping their rate calculation shared.

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
