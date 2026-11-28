# AGENTS.md

Load only the standards that match the change. The trigger is what the change affects, not the tool or the initial file list. If the scope expands, load any newly applicable standard before continuing.

| When the task changes | Read before making the change |
| --- | --- |
| Ownership, dependencies, application services, configuration or runtime state, permissions, plugins, or native side effects | [Architecture and ownership](.engineering/architecture.md) |
| Desktop UI, including layouts, shared components, styling, user-facing copy, or interaction behavior | [Desktop UI design standard](.engineering/ui-design.md) |
| Behavior, contracts, tests, CI, or acceptance criteria | [Verification and review](.engineering/verification.md) |

Use the applicable existing checks. Keep tool-specific instructions as pointers to these standards, not copies. A text-only edit that does not affect any of the areas above does not require loading the linked standards.
