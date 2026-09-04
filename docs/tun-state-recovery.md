# TUN state and restart recovery

The runtime `tun_status` query is the authority for whether TUN is running.
Transport failures and malformed replies propagate as errors; capability metadata
does not supply an OFF snapshot. The UI retains the last observation, labels it
unknown, and reads the saved preference separately. Pending status reads cannot
overwrite newer observations or cross a managed restart/mutation boundary.

For app-owned TUN, an explicit OFF saves `tun.enabled=false` before sending stop.
If stop cannot be confirmed, the operation reports failure while the saved OFF
prevents a later Core generation from restoring the previous ON intent. A stop
acknowledgement alone is insufficient: runtime status must confirm OFF. Profiles
that explicitly define `runtime.tun` retain ownership, including `null`.

The switch stays actionable when the saved preference is ON but the runtime is
unknown/stopped. Its explanatory text distinguishes saved intent from observed
state. Core start/restart returns its existing flattened process fields plus
`tunRestoreError` (null on successful/skipped restoration). The UI warns when
Core starts but TUN restoration fails, instead of reporting unconditional success.
Restoration submits at most one start command and polls status after transient
IPC failures, so a lost reply cannot trigger overlapping starts.

Validation includes production service/store behavior with controlled IPC,
Rust status decoding and restoration tests, and the existing cross-mode checks.
Real Wi-Fi/Ethernet switching with the paired Core DHCP permit change remains a
target-machine acceptance gate; these regressions do not establish live network
recovery or replace the currently running client/Core binary.
