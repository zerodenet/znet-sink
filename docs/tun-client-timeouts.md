# Client TUN response deadlines

TUN device/route setup is a control-plane operation, not a node connection
probe. Its duration must not use the ordinary two-second IPC query budget.

The Zero command adapter applies a minimum 60-second **response** budget to
`tun.start`, `tun.stop`, and `tun.recover`. This covers manual GUI toggles, automatic restoration
after Core start/restart, and legacy adapter calls. A longer explicitly configured
timeout is preserved; invalid timeouts are still rejected. IPC connect/subscribe
and ordinary query deadlines are unchanged. Shutdown retains its separate
four-second best-effort cleanup deadline before stopping the managed process.

Startup/restart/upgrade restoration derives its overall deadline from that same
command budget, five connection/status IPC budgets, and an eight-second final
reconciliation window. With the default two-second IPC timeout, this is 78
seconds. The former independent eight-second restoration timeout could abort
the command wait and force an upgrade rollback before device/route setup finished.

An expired restoration reports that the wait ended and asks the user to cancel
any residual system authorization window. It does not claim the kernel's native
authorization prompt was revoked: the current TUN command contract exposes no
authorization deadline or cancellation handle.

Previously only the GUI command wrapper applied the longer timeout; automatic
restoration bypassed that wrapper and could report failure after two seconds
while the kernel continued creating the device and installing routes.

The existing manual-toggle recovery still checks authoritative TUN status after
transient IPC errors before reporting failure or rolling back desired state.
An expired IPC wait is not cancellation or proof that TUN stopped. The command
layer does not retry mutations automatically.

Verification:

- Rust virtual-time/channel tests cover a twenty-second reply, a missing reply,
  unchanged ordinary-query timeout, custom longer timeout, and invalid values.
- The frontend source contract guards the shared adapter wiring and persisted
  TUN restoration path. It is not a live device test.
- Restoration tests cover slow setup, late completion after a lost reply,
  longer explicit budgets and the expiry notice without replaying a start.
- Rust tests run on GitHub-hosted Windows. Actual device/route timing and macOS
  acceptance remain user-run; local host network validation is not performed.
