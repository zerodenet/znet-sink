# Applying TUN settings while the client is running

The TUN settings page remains editable while an app-owned TUN is active.
Save and apply sends one serialized client transaction. The client and Zero
process stay running; current Zero versions stop and recreate the TUN device
to apply interface or capture-route parameters. Existing TUN connections can
be interrupted. This does not implement in-place route replacement in Zero.

The transaction validates a detached candidate, reads the live core identity
and detailed TUN parameters, stops the matching app-owned TUN, starts the new
one, verifies its parameters, and only then persists the candidate. Failure
restores the previous runtime and persisted settings when ownership and
command completion can be verified. An unresolved IPC timeout is not retried
and does not trigger a competing rollback; the UI reports the unverified
state. Core replacement, config revision changes, and external TUN changes
also prevent an unsafe rollback.

- Changing defaults while TUN or Zero is stopped does not start either one.
- A profile with `runtime.tun`, including an explicit null, keeps ownership.
  Saving local defaults does not modify that profile or its running TUN.
- Saving parameters preserves the user's persisted enabled/disabled intent.
- A runtime that omits include/exclude CIDR metadata cannot safely verify this
  transaction; update the kernel or use the existing stop/save/start flow.
- Clearing an optional interface name or secondary address removes its old value.

Verification includes Rust fake-runtime tests for success, rollback, disk-write
failure, lost replies, unresolved timeouts, ownership changes, and inactive
defaults, plus Chromium/WebKit fixture tests of the actual settings component.
The UI fixture never starts a kernel or changes host routes. Real privileged
TUN device and route acceptance remains a separate deployment check.
