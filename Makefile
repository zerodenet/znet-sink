# ZNet Sink Makefile
# =============================================================================
# Common development tasks and release workflow.
#
# This file is designed for GNU Make on any platform that has Git Bash
# (Windows with Git for Windows, macOS, Linux).

.PHONY: help dev frontend tauri build check test release update-version repair-manifest

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
	@echo "  make update-version   Release VERSION=x.y.z[-rc.N]; FORCE=1 retags"
	@echo "  make repair-manifest  Re-publish valid latest.json for a release (with TAG=v0.0.5)"
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
# Usage: make update-version VERSION=0.1.0-rc.1
# Retag the current version: make update-version VERSION=0.1.0 FORCE=1
# Auto-detects platform — PowerShell on Windows, bash elsewhere.
update-version:
ifeq ($(VERSION),)
	@echo "ERROR: VERSION is required — usage: make update-version VERSION=x.y.z"
	@exit 1
endif
ifeq ($(OS),Windows_NT)
	$(if $(filter 1,$(FORCE)),powershell -ExecutionPolicy Bypass -File scripts/update_version.ps1 "$(VERSION)" -Force,powershell -ExecutionPolicy Bypass -File scripts/update_version.ps1 "$(VERSION)")
else
	$(if $(filter 1,$(FORCE)),bash scripts/update_version.sh "$(VERSION)" --force,bash scripts/update_version.sh "$(VERSION)")
endif

# ── release: repair broken updater manifest for an existing release ──────
# Usage: make repair-manifest TAG=v0.0.5
# Re-publishes a valid latest.json so installed clients stop failing
# update checks.  Requires GitHub CLI (gh) authenticated.
repair-manifest:
ifeq ($(TAG),)
	@echo "ERROR: TAG is required — usage: make repair-manifest TAG=v0.0.5"
	@exit 1
endif
	bash scripts/repair_updater_manifest.sh "$(TAG)"
