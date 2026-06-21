# Current Integration Status

Updated: 2026-06-21

## Kernel Lifecycle

- `gui_disconnect` disables system proxy only and leaves the managed kernel running.
- Routine UI actions expose start and restart, not ordinary stop. `core_process_stop` is not registered as a Tauri command; internal stop remains for restart, shutdown, config change, and maintenance paths.
- Starting the kernel without an active proxy config uses a minimal temporary control-plane config where possible. Enabling system proxy still requires active proxy config content.

## IPC Contracts

- Query responses are unwrapped by variant key before parsing, including `health`, `capabilities`, `active_flows`, `flow`, `stats`, `policies`, and `tun_status`.
- `queryFlows()` sends `{ active_flows: { limit: 100, filter: {} } }` and parses `active_flows` / `activeFlows` response containers.
- TUN status uses the documented `tun_status` query path. The GUI adapter does not fallback to a `tun.status` command.

## Endpoints

- External Unix daemon default: `~/.zero/control.sock`.
- GUI-managed Unix kernel: executable-adjacent `zero-control.sock`, passed explicitly with `--control-socket`.
- Windows uses the configured named-pipe endpoint.

## Runtime Controls

- Proxy mode writes the kernel-native top-level `mode`.
- Legacy `route.mode` is accepted only as an import/read fallback.
- Runtime/control commands currently exposed through the adapter include `config.apply`, `config.validate`, `config.plan_apply`, `mode.set`, `diagnostics.dns_lookup`, `diagnostics.trace_route`, `recent_flows`, `sinks`, and `diagnostics`.

## Events And Capabilities

- GUI/core event subscriptions reconnect in the backend service layer with 1s, 2s, 4s, then 5s backoff.
- GUI event status carries a best-effort resync snapshot for `runtime`, `stats`, and `policies` after subscription recovery.
- Capabilities DTOs include `protocols` and `buildFeatures`; protocol entries expose TCP/UDP inbound/outbound support, MUX, status, and limitations.

## Verification

- `pnpm check`
- `cargo test --manifest-path src-tauri/Cargo.toml`
