# ZNet Sink Makefile
# =============================================================================
# Common development tasks and release workflow.
#
# This file is designed for GNU Make on any platform that has Git Bash
# (Windows with Git for Windows, macOS, Linux).

.PHONY: help dev frontend tauri build check test release update-version

# ── default target ───────────────────────────────────────────────────────
help:
	@echo "ZNet Sink — development targets"
	@echo ""
	@echo "  make dev              Start Vite dev server (frontend only)"
	@echo "  make tauri-dev        Start full Tauri dev (Rust + frontend)"
	@echo "  make build            Build frontend for production"
	@echo "  make tauri-build      Build installable Tauri app bundle"
	@echo "  make check            Typecheck (svelte-check)"
	@echo "  make test             Run Rust integration tests"
	@echo "  make update-version   Bump version, commit, tag, push (with VERSION=x.y.z)"
	@echo ""

# ── development ──────────────────────────────────────────────────────────
dev:
	pnpm dev

tauri-dev:
	pnpm tauri dev

# ── build ────────────────────────────────────────────────────────────────
build:
	pnpm build

tauri-build:
	pnpm tauri build

# ── quality ──────────────────────────────────────────────────────────────
check:
	pnpm check

test:
	cd src-tauri && cargo test

# ── release: bump version, commit, tag, push ─────────────────────────────
# Usage: make update-version VERSION=0.1.0
# Auto-detects platform — PowerShell on Windows, bash elsewhere.
update-version:
ifeq ($(VERSION),)
	@echo "ERROR: VERSION is required — usage: make update-version VERSION=x.y.z"
	@exit 1
endif
ifeq ($(OS),Windows_NT)
	powershell -ExecutionPolicy Bypass -File scripts/update_version.ps1 "$(VERSION)"
else
	bash scripts/update_version.sh "$(VERSION)"
endif
