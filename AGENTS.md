# AGENTS.md

## Project Overview
- Tauri 2 desktop app with SvelteKit 2 frontend
- TypeScript 6 + Tailwind CSS 4 + shadcn-svelte components
- Uses pnpm as package manager

## Commands
```bash
pnpm dev          # Start Vite dev server (http://localhost:1420)
pnpm build        # Build frontend for production
pnpm check        # Typecheck with svelte-check
pnpm tauri dev    # Start full Tauri dev environment (Rust + frontend)
pnpm tauri build  # Build full Tauri app bundle
```

## Architecture
- **Frontend**: `src/` - SvelteKit SPA (adapter-static with fallback index.html)
  - `src/routes/` - SvelteKit routes
  - `src/lib/` - Shared code
    - `components/ui/` - shadcn-svelte components
    - `components/` - Custom components
    - `hooks/` - React-style hooks
    - `services/` - Business logic
    - `types/` - TypeScript types
    - `utils.ts` - Utility functions (cn, etc.)
- **Backend**: `src-tauri/` - Rust Tauri backend
  - `src/main.rs` - Tauri app entry
  - `src/lib.rs` - Rust library code
  - `Cargo.toml` - Rust dependencies
  - `tauri.conf.json` - Tauri app config

## Important Notes
- Tauri requires SPA mode: uses `adapter-static` with fallback `index.html`
- Vite dev server runs on fixed port 1420 (strictPort: true)
- Vite ignores watching `src-tauri/**` to avoid Rust compile issues
- Window title: "ZNet Sink" (900x650 default size)
- shadcn-svelte aliases configured in `components.json`
- Package name: `org.zerdenet.znetsink`

## Code Style
- Svelte 5 Runes syntax
- TypeScript strict mode enabled
- Tailwind CSS v4 with @tailwindcss/vite plugin
- Use `$lib/` imports for shared code (configured by SvelteKit)
