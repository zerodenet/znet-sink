# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Develop Commands

```bash
pnpm install           # Install dependencies
pnpm dev               # Vite dev server (http://localhost:1420)
pnpm build             # Build frontend for production
pnpm check             # Typecheck with svelte-check
pnpm tauri dev         # Full Tauri dev (Rust + frontend)
pnpm tauri build       # Build installable Tauri app bundle
```

## Rust Backend Commands

```bash
cargo build            # Build from src-tauri/ (cd src-tauri)
cargo test             # Run Rust integration tests (cd src-tauri)
cargo test --test core_tests    # Core IPC tests only
cargo test --test domain_tests  # Domain config tests only
```

## Pre-commit Hooks (Husky)

- `pre-commit` — runs `pnpm check` (typecheck)
- `commit-msg` — runs `commitlint` against conventional commits

## Architecture

**Tauri 2 desktop app** — Rust backend manages lifecycle, an external proxy engine ("core"), and persistent config; SvelteKit SPA frontend renders the UI and communicates via Tauri IPC.

### Frontend (`src/`)

- **SPA mode**: `adapter-static` with fallback `index.html` (no SSR — required by Tauri). `src/routes/+layout.ts` sets `ssr = false`.
- **`$lib/components/ui/`** — shadcn-svelte primitives (currently only `tabs`). Add more via `pnpm dlx shadcn-svelte@latest add <component>`.
- **`$lib/components/`** — Application components (TitleBar, Sidebar, TabContent, TrafficChart, StatsGroup, etc.) and organized subdirectories: `core/`, `settings/`, `stats/`, `tabs/`.
- **`$lib/services/`** — Thin wrappers over `invoke()` from `@tauri-apps/api/core` that call named Tauri commands with correct parameter shapes matching Rust models.
  - `core.ts` — Core process lifecycle (`getCoreProcessStatus`, `startCoreProcess`, `stopCoreProcess`), IPC calls (`pingCore`, `queryCore`, `commandCore`, `getCoreStats`, etc.), event stream start/stop, app config get/update, logs, GUI capabilities snapshot.
  - `config.ts` — Domain CRUD: proxy configs (upsert, import, set active, remove), subscriptions (upsert, sync, remove), rule sets (upsert, remove).
- **`$lib/services/store.svelte.ts`** — Svelte 5 reactive singleton (`AppStateStore`) holding UI mode (lite/pro), active tab, theme, and visible tab list. `loadFromBackend()` syncs from Rust `AppConfig` on startup; `persistTheme()` and `switchUIMode()` push changes back to the Rust backend via `updateAppConfig`. Falls back to `localStorage` when the backend isn't available.
- **`$lib/services/theme.svelte.ts`** — Applies light/dark/system by toggling `.dark` on `<html>`. `setTheme()` calls `store.persistTheme()` to sync to Rust backend.
- **`$lib/types/`** — TypeScript interfaces mirroring Rust models, organized by domain:
  - `app-config.ts` — `AppConfig`, `AppCoreConfig`, `AppUiConfig` (includes `theme` and `uiMode`), `AppLogConfig`, `AppLocalProxyConfig`, and all `*Patch` types.
  - `core.ts` — `CoreProcessStatus`, `CoreEndpoint`, `CoreCallResult`, `CoreEventSubscription`, `CoreConfigSnapshot`, `CoreIpcOptions`, `AppError`.
  - `domain.ts` — `ProxyConfigProfile`, `ProxyConfigUpsert`, `ProxyConfigImport`, `ProxyConfigCapabilities`, `SubscriptionProfile`, `SubscriptionUpsert`, `RuleSetProfile`, `RuleSetSource`, `RuleSetUpsert`.
  - `logs.ts` — `LogEntry`, `LogAppend`, `LogQuery`, `LogSource`, `LogLevel`.
  - `capability.ts` — `CapabilityItem`, `GuiCapabilitySnapshot`.
  - `protocol.ts` — UI-level display types only (e.g. `ProxyNode`).
- **`$lib/constants/navigation.ts`** — Tab definitions with role-based visibility.
- **`src/app.css`** — Tailwind CSS v4 entry (no `tailwind.config.js` needed).

### Rust Backend (`src-tauri/src/`)

Layered architecture, top to bottom:

| Layer | Module | Responsibility |
|-------|--------|---------------|
| **Commands** | `commands/` | Tauri `#[command]` handlers — thin, delegates to services |
| **Services** | `services/` | Business logic, state mutation, persistence |
| **Models** | `models/` | `Serialize`/`Deserialize` data structs |
| **Core IPC** | `core/ipc.rs` | Raw JSON-line protocol over Unix sockets (macOS/Linux) or named pipes (Windows) |
| **Events** | `events/` | Tauri event emitters for streaming core events to the frontend |
| **State** | `state/` | `AppState` — all app data behind `Mutex` (config, domains, logs, core process handle) |
| **Errors** | `errors.rs` | `AppError` with `code`/`message`/`details`, typed constructors, `AppResult<T>` alias |

**Key service modules:**

- `control_plane` — Sends JSON-RPC-style frames (`{"type":"ping"|"query"|"command"|"subscribe",...}`) to the external core process via `core::ipc`. All core communication flows through here.
- `core_process` — Spawns/stops/polls the external core proxy binary as a child process. Reads `AppCoreConfig` for executable path, working dir, and config path. Logs lifecycle events.
- `core_events` — Opens a persistent subscription socket to the core, forwards each event line to the Svelte frontend via `emit_core_event`. Runs in a `spawn_blocking` thread. Tracks generation counter so old subscriptions can be stopped.
- `domain_store` — JSON file persistence for proxy configs, subscriptions, and rule sets in platform-specific data directories (`~/.config/znet-sink/` on Linux/macOS, `%APPDATA%/ZNet Sink/` on Windows, overridable via `ZNET_SINK_DATA_DIR`).
- `app_config_store` — Equivalent persistence for `AppConfig` (theme, UI mode, core settings).
- `logs` — In-memory log buffer with append/list/clear.

**Data directory** — Determined at startup (`domain_store::data_dir()`): respects `ZNET_SINK_DATA_DIR` env var, then `APPDATA` on Windows, then `XDG_CONFIG_HOME`, then `~/.config/znet-sink`, falling back to `.znet-sink` in CWD.

**Core binary** — The self-developed zero proxy engine binary lives at `build/core/zero` (referenced by `CoreConfigSnapshot::resolve_executable_path` in `services/core_config.rs`).

### Lite/Pro Mode

- `AppUiConfig` holds `theme` ("light"|"dark"|"system") and `ui_mode` ("lite"|"pro") — persisted to `app-config.json` via the Rust backend.
- The frontend store syncs both from the backend on startup (`store.loadFromBackend()`), falling back to localStorage.
- Switching modes in the UI calls `store.switchUIMode()` which updates the reactive state and pushes to Rust via `updateAppConfig`.
- Theme changes flow through `setTheme()` → `store.persistTheme()` → `updateAppConfig({ ui: { theme } })`.

### Startup Flow

1. `main.rs` → `gui_lib::run()`
2. `run()` loads app config and domain data (proxy configs, subscriptions, rule sets) from disk
3. Creates `AppState` with all data behind `Mutex`
4. Registers ~40 Tauri commands and the system tray (show/hide window, quit)
5. If `auto_start` is enabled in config, spawns the core process via `core_process::start()`
6. Window close is prevented — window hides to tray instead

### System Tray

- Left-click toggles window visibility
- Menu: "Show" (restore window) / "Quit" (exit app)
- Window decoration disabled (`decorations: false` in tauri.conf.json) — custom title bar via `TitleBar.svelte`

## Code Style

- **Svelte 5 runes** syntax (`$state`, `$derived`, `$effect`)
- **TypeScript 6 strict mode** — all `.ts`/`.svelte` files typed
- **Tailwind CSS v4** via `@tailwindcss/vite` plugin (no separate config file)
- **shadcn-svelte** aliases: `$lib/components/ui`, `$lib/utils`
- **Rust** edition 2021, standard formatting, `gui_lib` as lib crate name (avoids Windows name collision)
- **Import aliases**: `$lib/` maps to `src/lib/` (configured by SvelteKit)
