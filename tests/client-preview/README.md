# Client overview comparison and integration fixture

This is an isolated browser fixture, not a redesigned application or a release change.

- Title bar, application header, logo, navigation constants, cards, controls, and chart are imported from `src/`.
- The original overview is loaded verbatim by Vite from commit `1f3f6d6` (before `a3a95c8`), without modifying its template or CSS.
- The default client implementation is imported directly from `src/lib/components/tabs/OverviewTab.svelte`, including its real production overview controller.
- The wrapper follows the actual `src/routes/+page.svelte` desktop shell, plus a 29px fixture toolbar.
- `publicDir` points to the existing `static/`; both logo files are unchanged.
- Services and native window calls are mocked only by this Vite config. No real IPC, proxy/TUN operation, download, installation, or process restart is performed.
- Only the overview is loaded. The complete original navigation remains visible; other pages show a fixture notice. This does not represent removal of any client capability.
- Data, resource usage, IPs, and statuses are simulated. Original rendering behavior (including defects) is intentionally preserved for comparison.

Run with Node >=22.12:

```sh
pnpm exec vite --config tests/client-preview/vite.config.ts
```

Open `http://127.0.0.1:4178/overview-preview.html`.

Use the bottom selector to compare the client implementation, accepted design, and original overview. The original light/pro mode switch and the fixture theme switch are interactive. Backend actions are simulated.

## Optimized candidate (2026-09-06)

The accepted design is retained in `OptimizedOverview.svelte`. The default now loads the production overview through `OverviewTab.svelte`; the accepted design and original remain comparison choices.

- Reuses the actual title bar, logo, navigation, core controls, TUN Switch, chart, select, segmented control, and dialog components. `TunControl.svelte` preserves the existing feature-card style in a compact preview arrangement.
- Replaces the repeated status strip with active configuration and compact version access.
- Uses three control columns at the normal client width; core endpoint receives a full-width row so it is not truncated.
- Combines local-network and self-test summaries, with expandable capture/DNS/egress inspection.
- Adds policy-group selection and targeted actions for simulated TUN/selected-node failures.
- Shows the selected policy member and recent latency directly, with failure/stale labels and a direct-mode indicator.
- Keeps TUN health and its switch visible; capture details contain address, MTU, actual egress, and network generation. An enabled but unhealthy TUN is explicitly marked as abnormal.
- Configuration, mode, policy and capture changes wait for a simulated acknowledgement. Pending controls are disabled; failures keep the confirmed value and show a local error. Success messages expire after 3.5 seconds.
- The `切换失败` fixture scenario exercises unsuccessful commands without changing confirmed state.
- All candidate operations remain in the fixture state. They are not production command wiring or proof of network recovery.

Manual browser validation: configuration selection; failed selected node -> backup selection -> issue cleared; TUN failure -> recheck keeps failure -> disabling TUN leaves system proxy enabled. At 900x650 the normal overview has 0 horizontal/vertical overflow; at 640x650 horizontal overflow is 0. Standalone Vite build passed.

Polish validation: initial configuration label, pending/failed configuration, mode and policy selection, failed TUN toggle, successful TUN disable, direct-mode summary, and successful configuration switching. Confirmed state stays unchanged during pending/failed commands. Normal and dark 900x650 overflow is `{x: 0, y: 0}`; 640x650 is `{x: 0, y: 153}` (vertical scrolling). No page JavaScript errors were observed. The final standalone build passed.

## Production integration

The `客户端实现` choice mounts the real `OverviewController`, `ProfessionalOverview`, core resource card, TUN controls, and dialogs. Mock service adapters provide the same command-result shape, configuration acknowledgement, and ongoing traffic samples. The toolbar's failure scenario tests the production page's waiting/error presentation. It does not exercise the native backend.

Production model/store regression coverage lives in `scripts/test-professional-overview.mjs` and `scripts/test-overview-commands.mjs`; the latter loads the actual store with command/query adapter replacements. Production view interaction coverage lives in `tests/ui-controls/overview.spec.ts`. Run `pnpm test:overview`, `pnpm check`, `pnpm build`, and the overview browser tests.
