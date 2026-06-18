# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Develop Commands

```bash
pnpm install           # Install dependencies
pnpm dev               # Vite dev server (http://localhost:1420)
pnpm build             # Build frontend for production
pnpm check             # svelte-kit sync + svelte-check typecheck
pnpm commit            # Interactive conventional-commit prompt (commitizen)
pnpm tauri dev         # Full Tauri dev (Rust + frontend)
pnpm tauri build       # Build installable Tauri app bundle
```

## Rust Backend Commands

Run from `src-tauri/`:

```bash
cargo build                          # Build backend
cargo test                           # All integration tests
cargo test --test core_tests         # Core/kernel IPC tests
cargo test --test domain_tests       # Domain config + store tests
cargo test --test zero_config_tests  # Zero static-config parsing
cargo test --test zero_parsing_tests # Zero IPC response parsing
cargo test --test zero_snapshot_tests# Zero traffic snapshot math
```

## Pre-commit Hooks (Husky)

- `pre-commit` — runs `pnpm check`
- `commit-msg` — `commitlint` against conventional commits (see `COMMIT_CONVENTION.md`)

## Architecture

**Tauri 2 desktop app.** A Rust backend manages lifecycle, an external proxy engine ("kernel"), OS system-proxy settings, and persistent config. A SvelteKit SPA renders the UI and calls ~95 Tauri commands via `invoke`. Product name: **ZNet Sink**; the only kernel currently supported is the self-developed **Zero** proxy engine.

### Kernel Adapter Layer — the central abstraction

All kernel IPC lives in `src-tauri/src/kernel/` and is **kernel-agnostic by design**. Adding a kernel (sing-box, clash, …) means implementing one trait in a new `kernel/<name>/` sub-module.

```
kernel::adapter   (KernelAdapter trait — uniform GUI-facing operations)
kernel::zero/     (ZeroAdapter: the only implementation today)
  adapter queries commands parsing config events
kernel::protocol  (JSON-line IPC: frame encoding, endpoint/timeout resolution)
kernel::transport (socket/pipe I/O — Unix socket / Windows named pipe)
kernel::connection (one multiplexed long-lived connection — see below)
```

- `KernelAdapter` maps a kernel's native protocol into a single set of operations (health, capabilities, traffic, policies, connections, config apply/validate, mode switch, DNS/TUN/stack/rule status, diagnostics). GUI services depend only on this trait, never on a concrete kernel.
- **Multiplexed connection model**: the kernel keeps a connection open only if its first frame is `subscribe`. So `kernel::connection` opens *one* long-lived connection, sends `subscribe` first, then reuses it for every `query`/`command`/`ping` (each with a unique `id`). A background reader routes frames: top-level `ok` → response (paired by `id` to a waiting oneshot); no `ok` → event (broadcast); `:`-prefixed line → heartbeat. At most one connection per endpoint path, held by a global manager; on death every waiter is drained with `connection_closed` and the next call rebuilds it.

### Command surface — two layers

Commands are grouped into two distinct APIs (registration in `lib.rs`):

- **`core_*` — low-level IPC passthrough** (`commands/core.rs`, `commands/core_process.rs`, `commands/core_config.rs`): `core_ipc_ping/query/command/request`, `core_get_*` (capabilities/health/config/runtime/stats/policies), `core_select_policy`, `core_probe_policy`, `core_close_flow`, `core_events_start/stop`, `core_process_start/stop/restart/status`, `core_config_get/export_active/download_latest`. Thin wrappers that speak the kernel's raw protocol.
- **`gui_*` — high-level, adapter-mediated** (`commands/gui_core.rs`, `commands/gui_connection.rs`, `commands/gui_events.rs`, `commands/gui_self_test.rs`, `commands/proxy_mode.rs`): the **primary UI-facing API**. `gui_core_*` exposes ~40 composed operations (overview, traffic stats/snapshot, policy groups, proxy nodes, connections, DNS/TUN/stack/rule status, config apply/validate/plan, mode switch, probes, diagnostics, sinks). `gui_connect/gui_disconnect/gui_connection_status` drive the connection lifecycle; `gui_events_*` is a separate event stream.

Domain CRUD lives in its own commands: `proxy_config_*`, `subscription_*` (+ `subscription_sync_all`), `rule_set_*`, `app_config_get/update`, `logs_*`, `system_proxy_enable/disable/status`, `kernel_list_versions/install_version/detect_version`, `gui_capabilities_snapshot/interaction_surface_snapshot`, `gui_debug_frames/clear`.

All commands return `Result<T, AppError>`. Error codes (`invalid_argument`, `not_found`, `core_unavailable`, `timeout`, `connection_closed`, `mode_restricted`, …) are documented in [docs/gui/README.md](docs/gui/README.md).

### Rust Backend (`src-tauri/src/`)

| Module | Responsibility |
|--------|----------------|
| `commands/` | Tauri `#[command]` handlers — thin, delegate to services |
| `services/` | Business logic, state mutation, persistence, kernel/process/system-proxy control |
| `kernel/` | Kernel adapter abstraction + Zero implementation + IPC transport (above) |
| `lifecycle/` | Phased startup (`Guard→Config→State→Register→Runtime`) + coordinated reverse-order shutdown |
| `models/` | `Serialize`/`Deserialize` structs |
| `events/` | Tauri event emitters for streaming kernel/GUI events to the frontend |
| `state/app_state.rs` | `AppState` — all app data behind `Mutex`, event generation counters, traffic sample, `ManagedCoreProcess` |
| `errors.rs` | `AppError` (`code`/`message`/`details`), typed constructors, `AppResult<T>` |

Key services: `core_process` (spawn/stop the kernel binary, also `kill_external` when we don't own the PID), `core_events` (persistent subscribe socket → frontend), `core_config` (resolve paths/endpoints, export, download latest), `kernel_manager` (kernel selection), `local_proxy` (wait for local listener readiness), `system_proxy` + `system_proxy_guard` (set/restore OS system proxy with cleanup-on-exit), `probe` (latency), `proxy_mode` + `interaction_mode` (lite/pro gating), `gui_connection` / `gui_self_test` / `gui_events` / `capability`, `domain_store` + `app_config_store` (JSON persistence), `subscription` (incl. `spawn_auto_sync_scheduler`), `log_store`/`logs`.

### Lifecycle (`src-tauri/src/lifecycle/`)

`run()` drives a phased system (`Phase` enum, `Ord`-sorted). Startup prints `[ZNet] lifecycle:` progress; shutdown runs registered callbacks in **reverse phase order** (e.g. `stop_core_process` then `system_proxy_cleanup`). Shutdown guards are registered via `shutdown_coordinator_mut().register(phase, name, closure)`. New subsystems hook in here rather than ad-hoc in `run()`.

### Frontend (`src/`)

- **SPA mode**: `adapter-static` + fallback `index.html` (no SSR — Tauri requirement). `src/routes/+layout.ts` sets `ssr = false`.
- **`$lib/services/core.ts`** — wraps both `core_*` and `gui_*` commands (the main service). `config.ts` — domain CRUD. Other reactive stores: `store.svelte.ts` (UI mode/theme/active tab), `theme.svelte.ts`, `gui-state.svelte.ts`, `core-events.svelte.ts`, `updater.svelte.ts`, `kernel-version.ts`, `config-editor.svelte.ts`, `overview-data.svelte.ts`, `delay-history.svelte.ts`, `toast.svelte.ts`. All `.svelte.ts` files use Svelte 5 runes.
- **`$lib/types/`** — TypeScript interfaces mirroring Rust models, by domain (`core.ts`, `gui-api.ts`, `domain.ts`, `app-config.ts`, `logs.ts`, `capability.ts`, `kernel-version.ts`, `debug.ts`, `protocol.ts`).
- **`$lib/components/ui/`** — shadcn-svelte primitives (button, card, input, select, tabs, switch, badge, separator, Skeleton, Spinner). Add more via `pnpm dlx shadcn-svelte@latest add <component>`.
- **`$lib/components/`** — app components, organized into `core/`, `settings/`, `stats/`, `tabs/`, `WelcomeGuide/`. **`$lib/constants/navigation.ts`** — tab definitions with role-based visibility.
- **`src/app.css`** — Tailwind CSS v4 entry (no `tailwind.config.js`).

### Lite/Pro Mode & Interaction Modes

`AppUiConfig` holds `theme` and `ui_mode`. The frontend store syncs from the backend on startup (`loadFromBackend()`), falling back to localStorage; changes push back via `updateAppConfig`. The Rust `interaction_mode`/`proxy_mode` services gate pro-only operations and return the `mode_restricted` error code when a lite user hits a pro action — the frontend surfaces this with a toast (`handleAppError`). Menu/operation visibility comes from `gui_interaction_surface_snapshot` and `app_config_get().ui.hiddenMenuKeys`.

### Startup Flow

1. `main.rs` → `gui_lib::run()` → `lifecycle::phases::build_builtin()` runs Guard + Config phases (loads `AppConfig`, domain data, logs from disk).
2. `AppState` constructed with all domain data behind `Mutex`.
3. Tauri builder registers plugins (`dialog`, `opener`, `updater`) + ~95 commands + system tray.
4. In `.setup()`, an async task does a **fast kernel-alive probe** (200ms `ping` on a short-lived connection): if a kernel answers, the GUI connects to it (and restarts only if the running binary's path no longer matches config); otherwise it starts the managed kernel when `core.auto_start` is enabled.
5. `subscription::spawn_auto_sync_scheduler` starts background subscription re-syncs.
6. Window close is prevented — it hides to tray instead.

### System Tray

Left-click toggles window visibility. Menu: 显示/隐藏, 开启/关闭系统代理, 启动/停止/重启内核, 设置 (opens to settings/general), 退出. Status-dependent items' enabled state is toggled by the frontend-driven `tray_update_status` command. Window decoration disabled (`decorations: false`) — custom title bar via `TitleBar.svelte`.

### App Updater

`tauri-plugin-updater` is configured in [tauri.conf.json](src-tauri/tauri.conf.json) against GitHub releases (`zerodenet/znet-sink`, `latest.json` manifest) with minisign pubkey verification. Frontend updater logic is in `$lib/services/updater.svelte.ts`.

## Persistence & Data Directory

Resolved once by `services::data_dir()` (defined in `services/mod.rs`, single source of truth; imported as `super::data_dir` by the stores):

1. `ZNET_SINK_DATA_DIR` env var (dev override)
2. Platform path: Windows `%APPDATA%/ZNet Sink`; macOS/Linux `~/.config/znet-sink` (respecting `XDG_CONFIG_HOME`)

There is **no CWD fallback** — if none resolve it returns an error. `domain_store` persists proxy configs / subscriptions / rule sets; `app_config_store` persists `AppConfig` (theme, UI mode, core settings). The Zero kernel binary is referenced at `build/core/zero` (see `services/core_config.rs`).

## Front-Backend Contract Docs

[docs/gui/](docs/gui/) is the canonical contract between frontend and Rust backend (currently only the Zero kernel is documented there): boundaries, calling conventions, error codes, and per-domain references (app-config, interaction-modes, capabilities, zero-adapter, core, proxy-config, subscriptions, rule-sets, logs). Consult it before changing the command/model shapes.

## Code Style

- **Svelte 5 runes** (`$state`, `$derived`, `$effect`); `.svelte.ts` for reactive stores outside components
- **TypeScript strict** — all `.ts`/`.svelte` files typed
- **Tailwind CSS v4** via `@tailwindcss/vite` (no config file); shadcn-svelte aliases `$lib/components/ui`, `$lib/utils`
- **Rust** edition 2021, lib crate name `gui_lib` (avoids Windows name collision with the bin)
- **Import alias**: `$lib/` → `src/lib/`
