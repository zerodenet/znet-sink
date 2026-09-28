# Client TUN capture and host DNS

## Responsibility

The kernel answers DNS packets that actually enter TUN. The client installs the host resolver entry and owns its lifetime. A healthy TUN with automatic routes does not require system proxy to be enabled. IPC failure, unhealthy TUN and DNS capture failure remain actionable.

Only the client's current managed child is eligible. Before the first resolver change, the client checks the endpoint PID, requires independent kernel DNS upstreams, sends a DNS probe to the synthetic TUN peer (IPv4 address + 1), and confirms the kernel's interception counter advanced. Missing or unsupported configuration fails without installing a resolver.

The client reads effective profile/default DNS policy on every reconciliation. Failed installation is retained as an error and does not continually re-prompt. TUN recovery retries it. Disabling TUN or stopping the managed runtime removes the host DNS entry before stopping TUN.

## Platforms

| Platform | Entry | Cleanup and restrictions |
| --- | --- | --- |
| macOS | A private SystemConfiguration session DNS resolver | Existing authorized helper; session keys disappear when the helper exits; no persistent network-service preferences change |
| Windows | A dedicated NRPT root namespace rule | Requires Administrator privileges already required for TUN; rejects existing root DNS policies; guardian removes only its identified rule on lifetime pipe closure or kernel exit |
| Linux | systemd-resolved per-link DNS and routing domain ~. | Requires host resolver stub 127.0.0.53 and resolved authorization; rejects existing per-link DNS/domain settings; removes only its DNS/domain values; never rewrites /etc/resolv.conf |
| Other platforms | Unsupported | Explicit error; no host resolver mutation |

Private guardian arguments are validated; Windows retains a process handle and Linux compares process start time to avoid PID reuse. Foreign resolver changes are preserved. Cleanup failure is reported instead of claiming restoration.

NRPT remains a persistent Windows store: forced termination of the guardian itself or power loss can leave a rule. The next installation removes only recognizable stale client rules after checking owner PID and start time; it rejects live or unrecognized managed rules.

## Acceptance

Automated coverage checks target validation, independent-upstream requirements, install failure caching, restore-before-replace, retry after cleanup failure, TUN-only readiness, retained genuine errors, and SVG flag presentation without changing node/policy IDs.

Live checks must separately confirm: TUN only with system proxy off; correct system A/AAAA resolution; browser TLS validation; DNS capture status; disabling TUN; changing network; stopping/crashing client; original resolver restored; foreign VPN settings preserved. Kernel DNS probe success alone does not prove host resolver takeover or browser recovery.

## References

- [Apple session resolver options](https://developer.apple.com/documentation/systemconfiguration/scdynamicstorecreatewithoptions(_:_:_:_:_:))
- [Microsoft NRPT rules](https://learn.microsoft.com/en-us/powershell/module/dnsclient/add-dnsclientnrptrule?view=windowsserver2025-ps)
- [systemd VPN resolver routing](https://systemd.io/RESOLVED-VPNS/)

## Changed client surfaces

- Readiness: `src-tauri/src/services/gui_self_test.rs`, `src/lib/components/overview/model.ts`, `src/lib/components/core/CoreStatusCard.svelte`, `src/lib/components/core/KernelStatusPill.svelte`.
- Flag presentation: `src/lib/components/FlaggedText.svelte`, `src/lib/services/flag-text.ts`, `src/lib/components/ui/select/field-select.svelte`, `src/lib/components/overview/ProfessionalOverview.svelte`, `src/lib/components/overview/OverviewDialogs.svelte`.
- DNS controller: `src-tauri/src/capture/dns.rs`, `src-tauri/src/capture/dns/{controller,platform,preflight,helper,tests}.rs`, `src-tauri/src/capture/dns/helper/native.rs`, `src-tauri/src/services/macos_privilege.rs`, `src-tauri/src/services/macos_privilege/tun_dns.rs`.
- Runtime wiring and status: `src-tauri/src/application/mod.rs`, `src-tauri/src/capture/mod.rs`, `src-tauri/src/capture/shutdown.rs`, `src-tauri/src/kernel/zero/runtime.rs`, `src-tauri/src/main.rs`, `src-tauri/src/models/zero_runtime.rs`, `src-tauri/src/runtime_host/{owner,stop}.rs`, `src/lib/types/gui-api.ts`.
- UI tests: `scripts/test-professional-overview.mjs`, `tests/ui-controls/OverviewFixture.svelte`, `tests/ui-controls/overview.spec.ts`.

No kernel files, dependency versions, release versions or unrelated plugin/subscription edits belong to this change.

## Verification recorded 2026-09-28

- Svelte check: zero errors and zero warnings.
- Overview logic tests: 23 passed.
- Browser overview regression: 16 passed on the first full run; one startup timeout passed on focused rerun, completing all 17 cases.
- GUI Rust capture/readiness/macOS helper/runtime tests: 33 passed (20 capture, 3 self-test, 4 macOS helper, 6 Zero runtime).
- Rust formatting and diff whitespace checks passed.
- Windows MSVC and Linux GNU targets: the production DNS guardian, native backend and platform guard modules passed isolated cross-compilation with their dependency surfaces. This is not a complete client build or native OS acceptance.
- Live macOS kernel: querying the TUN peer 10.0.0.2:53 returned Fake-IP 198.18.0.3; DNS hijack count advanced from 0 to 1. Host OS DNS was not changed during this probe.
- No new client installed or released. Real host DNS takeover/restoration and browser TLS recovery remain unverified; Windows/Linux native tests also remain pending.
