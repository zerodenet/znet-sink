# v0.0.3 Unified configuration management

Development line: `develop` (v0.0.3). New capabilities are implemented directly
on `develop`; v0.0.2 `main` receives no new feature implementations.

## Configuration identity and sources

Both layouts expose **Configuration** (`profiles`). The legacy `subscriptions`
route, including tray shortcuts and saved default routes, opens the same view.
Local JSON, files, URL subscriptions, and plugin-managed configurations share
one catalog and explicit activation flow. Sources are capabilities of a
configuration, rather than a second user-facing collection.

`configuration_list` projects metadata from the existing SQLite relationship
graph under the profile/subscription locks. It omits raw configuration JSON.
An existing profile retains its ID and appears once, even with multiple legacy
sources. All such sources remain individually editable/updatable/removable.
Unsynced and missing-target sources remain visible as pending entries. Names
are never used to infer associations. Creating a URL source doesn't expose a
target picker; updating its settings retains its original association.

The normalized tables and legacy IPC/SDK commands remain compatible. This is
an aggregate and lifecycle unification, not destructive replacement of data.
There is no mandatory redownload, ID regeneration, or network operation during
upgrade. The catalog doesn't claim a saved configuration was confirmed by the
running kernel: runtime confirmation remains in the configuration workspace.

## Selector preference upgrade

`ProxyConfigProfile.policySelections` is authoritative for all source kinds.
Old records default to an empty map. SQLite loading performs an additive,
idempotent upgrade in one transaction: preserve existing configuration choices,
then fill absent groups from linked sources in their original order. The old
source maps are retained, including conflicting historical values.

Successful explicit selector changes update the configuration and all attached
source mirrors in one SQLite transaction. URL source refresh preserves the
configuration choices. Composition uses the explicit profile ID, so equal JSON
contents in two profiles cannot transfer choices between them. Invalid choices
are ignored when composing a candidate, never applied to automatic groups.
Removing a source while retaining its configuration retains these preferences.

## Lifecycle and ownership

- Local editing/import, URL refresh, managed publication and activation keep
  their existing validated backend publication operations and rollback behavior.
- Both overview modes use the catalog and the same activation function. An
  explicit selection of a pending ordinary source updates once, then activates
  the generated profile. Failed updates never activate a different profile.
- A source saved before a failed first update remains visible. The create dialog
  disables resubmission after saving; retry uses the saved source ID.
- Inactive updates don't initiate TUN. Subscription sync doesn't issue a second
  frontend TUN reconciliation. Existing boxed async phase boundaries and stack
  regression tests remain in place.
- Explicit client global settings retain precedence across configurations.
  Source settings and choices are separate from global DNS/TUN/listener settings.
- Generic plugin identity, authorization, source namespace and applied revision
  checks are unchanged. Managed sources link to plugin management; ordinary
  source editing, URL refresh and removal cannot bypass their owner.
- Removal previews identify shared configurations. Removing one source keeps
  the configuration by default; deleting a shared target remains forbidden.
