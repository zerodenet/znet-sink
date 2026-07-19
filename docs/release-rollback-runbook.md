# Release Rollback Runbook

This document is the operational checklist for stopping and rolling back a
ZNet Sink desktop release. It applies to stable and prerelease channels.

## Release Preconditions

- `pnpm check`, frontend tests, `pnpm build`, and Rust tests pass.
- The release candidate completes the Windows proxy restore and uninstall test.
- The candidate starts with both a valid configuration and an empty first-run state.
- Configuration changes are backward compatible, or the previous files are
  backed up before migration.
- The previous known-good installers and updater manifest are retained.
- The release owner and rollback operator are identified before publishing.

## Staged Rollout

1. Publish a prerelease with `./scripts/update_version.sh 0.1.0-rc.1` and
   install it on internal machines. The script keeps the full prerelease SemVer
   for the app and updater while generating numeric MSI and macOS bundle
   versions required by those platforms.
2. Verify startup, kernel readiness, proxy enable/disable, subscription sync,
   update installation, restart, and clean shutdown.
3. Observe exported diagnostics and local error logs for at least one full
   update/restart cycle.
4. Promote the same commit to stable only after the checks above pass.

## Stop Conditions

Pause the rollout immediately when any of these is observed:

- the application cannot show its main window;
- the kernel repeatedly fails to start or reconnect;
- the system proxy is left enabled after disconnect, crash, exit, or uninstall;
- configuration written by the new version cannot be read by the previous version;
- the updater manifest is incomplete, unsigned, or points to missing artifacts;
- a fatal or repeated error affects a core user flow.

## Immediate Containment

1. Mark the affected GitHub release as prerelease or draft when possible.
2. Remove or replace `latest.json` so new clients no longer discover the bad release.
3. Do not delete the release artifacts until affected users have a recovery path.
4. Publish a short incident notice containing the affected versions and the
   recommended action.
5. Ask affected users to export diagnostics from Settings > About before reinstalling.

## Rollback Procedure

1. Confirm the last known-good application version and compatible Zero kernel version.
2. Back up the user's application data directory before changing versions.
3. Disable the system proxy and confirm the operating-system proxy state was restored.
4. Install the last known-good application package.
5. If the new release performed a non-compatible configuration migration, restore
   the pre-migration backup before starting the old application.
6. Start the old version and verify:
   - the main window renders;
   - the kernel becomes healthy;
   - the active configuration loads;
   - proxy enable/disable restores the original OS settings;
   - subscriptions and nodes remain readable.
7. Republish an updater manifest that points to the known-good or fixed version.

## Evidence To Preserve

- exported diagnostic directory;
- affected and previous application versions;
- Zero kernel version and capability snapshot;
- updater manifest and artifact signatures;
- operating-system and architecture;
- exact reproduction steps;
- whether configuration migration occurred;
- proxy state before, during, and after recovery.

## Follow-up

- Add a regression test for the failure.
- Record the root cause and why pre-release checks missed it.
- Update this runbook if containment or recovery required undocumented steps.
- Do not resume stable rollout until the failure is reproduced and the fix is
  verified through the same staged path.
