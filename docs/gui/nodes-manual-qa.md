# Nodes Manual QA

This checklist is for validating the current Nodes tab behavior in the Tauri desktop app after the recent refactor and batch probe fixes.

## Preconditions

- Launch the desktop app with `pnpm tauri dev`.
- Import and activate a profile that contains:
  - at least one selector group
  - at least one non-selector group such as `urltest` or `fallback`
  - multiple proxy nodes
- Prepare one case where the kernel is connected and one where it is disconnected.

## Batch Probe

1. Open the Nodes tab with the kernel connected.
2. Stay on `All Nodes`.
3. Click the batch probe button once.
4. Verify:
   - every node in the current filtered result enters probing state
   - the toolbar progress counter increases until completion
   - probing state clears when the batch completes
   - the batch button is disabled while probing is in progress

## Single Probe vs Batch Probe

1. Start a single-node probe.
2. While that node is probing, try to start a batch probe.
3. Verify:
   - the batch probe does not start
   - the existing single probe continues and completes normally
   - no node gets stuck in probing state afterward

## Group Switching

1. Switch between `All Nodes` and one selector group.
2. Trigger single-node selection inside the selector group.
3. Verify:
   - the selected node updates in the expected group
   - `All Nodes` still stays selected when you explicitly choose it
   - switching groups does not silently reset the sidebar back to the first group

## Non-Selector Group Guard

1. Open a non-selector group such as `urltest`, `fallback`, or `load-balance`.
2. Try to click a node card or row to switch it.
3. Verify:
   - manual switching is blocked
   - the UI shows a clear error instead of pretending the action succeeded

## Search + Probe Scope

1. Enter a search query so only part of the node list remains visible.
2. Start a batch probe.
3. Verify:
   - only the filtered nodes are included in the batch
   - hidden nodes do not enter probing state
   - clearing the search after completion shows stable final state

## Disconnected State

1. Disconnect the kernel or start from a disconnected state.
2. Open the Nodes tab.
3. Verify:
   - single probe buttons are disabled
   - the batch probe button is disabled
   - node switching actions are disabled
   - attempting an action through any remaining path shows the offline error message

## Core Ready But Proxy Off

1. Start the kernel so IPC is healthy, but keep the system proxy disabled.
2. Open the Nodes tab.
3. Verify:
   - single-node probe remains available
   - batch probe remains available
   - node switching remains available for selector groups
   - the page does not incorrectly require the full "service connected" state for node operations

## Shared Node Across Multiple Groups

1. Use a profile where the same outbound tag appears in more than one group.
2. Open `All Nodes`.
3. Find that shared node in its rendered section.
4. Switch it from the Nodes tab.
5. Verify:
   - the node appears under the expected first matching group section
   - the switch command applies to that same group context
   - UI grouping and actual selection target do not disagree

## Regression Sweep

- Toggle list/grid view during ordinary browsing.
- Change sort mode between delay and name.
- Expand and collapse sections in `All Nodes`.
- Confirm no visible Chinese text is garbled in:
  - app loading screen
  - startup failure screen
  - Nodes tab empty states
  - legacy node selector surfaces if they are reintroduced
