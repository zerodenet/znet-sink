# Current Integration Status

Updated: 2026-08-21

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
- Runtime/control commands currently exposed through the adapter include `config.apply`, `config.validate`, `mode.set`, `diagnostics.dns_lookup`, `diagnostics.dns_cache`, `diagnostics.fakeip_lookup`, `diagnostics.trace_route`, `diagnostics.probe_outbound`, `recent_flows`, `sinks`, and `diagnostics`. The current kernel contract does not expose `config.plan_apply`.

## DNS / TUN / Fake-IP Integration

- TUN runtime status and Zero's DNS/Fake-IP diagnostics remain authoritative for runtime behavior; the GUI only manages configuration and presents returned state.
- A dedicated settings surface supports Disabled, Real DNS, and Fake-IP modes, named UDP/DoH/DoT/DoQ/system servers, cache settings, Fake-IP lifecycle settings, ordered shared-condition DNS dispatch, and a validated native `runtime.dns` JSON editor.
- DNS/Fake-IP settings are owned by the client app, independent of proxy profiles. Legacy profile-owned `runtime.dns` is migrated once and removed; every effective Zero config receives the global setting at apply/export/profile-switch time.
- DNS changes are checked by the lossless client model and running kernel, then committed with last-known-good rollback. A running kernel still needs an active proxy profile as the transport config to apply the global DNS setting.
- App-owned TUN passes `dns_hijack` only when the global client DNS configuration is enabled. Profile-owned `runtime.tun` remains authoritative when present.
- Read-only diagnostics expose DNS cache entries, Fake-IP forward/reverse lookup, allocator counters, and the kernel-provided `original_ip`, `host_source`, and `fake_ip_reverse_status` flow fields.
- Port-53 interception does not claim coverage of application-owned DoH/DoT/DoQ or hostnames hidden by ECH.

## TUN / FakeIP Phase 1 Integration

- TUN runtime status remains the source of truth for GUI presentation. New kernel-side egress routing and DNS binding capabilities should be consumed as status/diagnostic data rather than duplicated in GUI state.
- The GUI integration path is prepared for read-only DNS and FakeIP diagnostics. Editing FakeIP pools or advanced TUN stack parameters is intentionally deferred until kernel contracts stabilize.
- Phase 1 scope:
  - TUN address/CIDR defaults follow kernel changes.
  - TUN runtime status exposes interface, address, and routing related information when available.
  - DNS/FakeIP diagnostics are integrated as read-only observability features.

## Events And Capabilities

- GUI/core event subscriptions reconnect in the backend service layer with 1s, 2s, 4s, then 5s backoff.
- GUI event status carries a best-effort resync snapshot for `runtime`, `stats`, and `policies` after subscription recovery.
- Capabilities DTOs include `protocols` and `buildFeatures`; protocol entries expose TCP/UDP inbound/outbound support, MUX, status, and limitations.

## Verification

- `pnpm check`
- `cargo test --manifest-path src-tauri/Cargo.toml`
