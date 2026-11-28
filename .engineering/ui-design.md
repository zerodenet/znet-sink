# Desktop UI design standard

This standard defines the visual design, layout, components, and interaction of the znet-sink desktop client. Feature designs own their specific workflows and business decisions.

## 1. Visual direction

Design a calm, readable desktop network tool with cool gray surfaces, an indigo accent, clear text hierarchy, and compact controls. Use alignment, spacing, and subtle dividers to group related content. Do not wrap every section in a card. Reserve elevation for menus, dialogs, and other actual overlays; avoid decorative glass effects, gradients, heavy shadows, and large blocks of accent color.

Use indigo for primary actions, selection, and keyboard focus. Reserve semantic colors for their intended states; do not reuse them as arbitrary chart categories. Lite and Pro share typography, visual language, and state semantics. Pro may use denser variants; it does not get a separate theme.

### Let the interface explain the task

- Use placement, grouping, alignment, selected states, and action hierarchy to convey structure. Give objects and controls precise labels. Do not add boilerplate introductions below page titles or descriptions below every card or switch. Do not repeat a state the interface already communicates.
- Keep labels for unfamiliar actions, value units, non-color state cues, the scope and consequences of an action, and information needed to fix errors. Show decision-critical information where it matters. Use icon-only controls only for familiar actions with unambiguous icons and accessible names. Tooltips may explain optional details; they must not carry required information.
- Show guidance when the user needs it. An empty subscription list can offer one actionable import prompt; a populated list does not need a permanent tutorial. The Connect button and connection state should not need an instruction to click the button. If an action affects device-wide traffic, explain that beside the action.

## 2. Color and themes

### Base and interaction colors

| Role | Light | Dark | Use |
| --- | --- | --- | --- |
| App background | `#F5F7FB` | `#10131B` | Base window background |
| Content surface | `#FFFFFF` | `#181D2A` | Content, tables, settings |
| Overlay surface | `#FFFFFF` | `#22293A` | Menus, popovers, dialogs |
| Primary text | `#182133` | `#EDF1F8` | Body text and headings |
| Secondary text | `#526079` | `#A9B5CA` | Supporting text and secondary information |
| Divider | `#DCE2EC` | `#343D50` | Visual separation only |
| Control boundary | `#75829A` | `#7684A0` | Identifies enabled inputs and controls; do not substitute the divider color |
| Primary action background | `#4358D6` | `#A9B6FF` | Primary buttons |
| Primary action foreground | `#FFFFFF` | `#161C3A` | Primary button text and icons |
| Selection background | `#E9EDFF` | `#2C355A` | Selected row, option, or tab |
| Selection foreground | `#263C9D` | `#D4DDFF` | Selected content |
| Focus ring | `#334BCC` | `#A9B6FF` | 2 px wide with a 2 px offset; add a contrasting separator when needed |
| Upload series | `#6553B8` | `#BAA7FF` | Label it and use a distinguishable line pattern |
| Download series | `#087E8B` | `#63D0DF` | Label it and use a distinguishable line pattern |

| State | Light foreground / background | Dark foreground / background |
| --- | --- | --- |
| Information | `#2946A3` / `#EDF2FF` | `#BBC8FF` / `#273255` |
| Success | `#16734E` / `#E7F6EF` | `#66D5A3` / `#17352B` |
| Warning | `#8A5300` / `#FFF1D6` | `#F5C36B` / `#392B16` |
| Error | `#B4233B` / `#FDECF0` | `#FF899A` / `#3B202B` |

Define hover, pressed, disabled, and scrim colors in the shared component layer and review them in component examples. Pages must not introduce their own palettes. Disabled, read-only, selected, stale, and failed states must be distinguishable without relying on color alone.

### Dark mode and theme changes

- Offer System, Light, and Dark, with System as the default. Store an explicit user preference locally. Respond to operating-system theme changes only in System mode.
- Apply the theme before drawing content. Switching themes must not reload the window, reset the page or filters, discard drafts, change proxy state, or move focus unnecessarily.
- Use the same components with different tokens. Dark mode must define distinct app, content, and overlay surfaces and cover menus, dialogs, tooltips, charts, validation errors, and in-app scroll regions. Do not invert colors.
- Verify rendered text against [WCAG 2.2 contrast minimums](https://www.w3.org/TR/WCAG22/#contrast-minimum): 4.5:1 for normal text and 3:1 for large text. Required [non-text boundaries and state indicators](https://www.w3.org/TR/WCAG22/#non-text-contrast) must reach 3:1. Use the same threshold for focus rings against adjacent surfaces. Palette checks alone do not establish rendered contrast.

## 3. Typography, sizing, and motion

Use logical pixels, the system UI font, and appropriate language fallbacks. Use tabular figures for live values and aligned numeric columns. Use monospace for addresses, identifiers, and raw technical text only when it improves readability.

| Element | Font size / line height, or size | Rule |
| --- | --- | --- |
| Page title | 24 / 32 | Weight 600 |
| Section title | 16 / 24 | Weight 600 |
| Body text and field label | 14 / 20 | Body 400, label 500 |
| Supporting text | 12 / 18 | Weight 400; do not rely on this style alone for critical errors |
| Metric value | 24 / 32 | Weight 600, tabular figures |
| Standard / compact control | Height 36 / 32 | Compact only in dense areas; default to standard for key actions |
| Standard / compact table row | Height 40 / 32 | Allow expansion for long text, validation messages, and larger type |
| Icon | 16 or 20 | Use one icon family; icon-only actions need accessible names |

Use a 4 px spacing grid: 4, 8, 12, 16, 24, 32. Default to 24 px page padding, 16 px panel padding, and 8 px between related controls. Use 8 px control radii and 12 px panel radii. Text and status changes must not move the primary action. Use 120–180 ms transitions when they help orientation. With reduced motion enabled, omit nonessential movement and chart transitions while preserving clear progress feedback.

## 4. Shared desktop layout

Use the shared app shell and a shared page template. The shell owns window navigation, page headers, content gutters, and the main scroll region. Pages fill slots for titles, actions, filters, content, and details. Do not build a second header, gutter system, or primary scroll region per page. Add a reusable template variant when the existing template cannot express a new structure.

Use the same alignment, title hierarchy, action placement, and narrow-window behavior for equivalent pages. Nodes, Subscriptions, and Connections use the same header, primary-action, and filter order, as well as consistent detail entry and return behavior. Task and overview pages may have different structures within the same shell. Follow platform window-control and keyboard conventions.

| Template | Required structure | Narrow-window behavior |
| --- | --- | --- |
| Task | Title, current state, clear next action, local recovery | No horizontal scrolling to reach the primary action |
| Overview | Title, persistent important notices, optional observation widgets | Widgets wrap into supported sizes; the primary action remains outside charts |
| List and detail | Title and primary action share a region; filters sit next to the list; selection and detail entry are explicit | Use a shared overlay panel or dedicated detail page when panes do not fit; provide a clear way back |
| Form and submit | Title, grouped fields, field-level validation, bottom action bar, and scope and outcome for explicit application | Form scrolls independently; the action bar stays visible; the last field and its error can scroll fully into view |

Review at 1200 × 800 and at 900 × 650 for a narrow window. These are review viewports, not hard-coded sizes or minimums. Reflow based on the available content width and allow text to wrap. Limit horizontal scrolling to data tables that genuinely need it. Preserve filters and selection when returning to a list.

## 5. Component system

Use shadcn as the foundation through the shared project component layer. Keep theme adjustments and common variants there. Do not copy primitive implementations into pages or add a visual variant for one page alone. This standard does not choose a framework or a shadcn port.

| Layer | Responsibility | Examples |
| --- | --- | --- |
| Primitives | Base behavior and styling | Buttons, switches, inputs, selects, badges, dialogs, menus, tabs, tooltips |
| Shared composites | Reusable composition, layout, and keyboard behavior | Data tables, filter bars, form groups, form action bars, detail panels, inline notices, widget containers |
| Shared domain components | Receive structured domain state and emit user intent | Connection controls, subscription import, node selection, configuration apply, network recovery |
| Page templates | Position titles, actions, and content; coordinate scrolling and narrow-window behavior | Task, overview, list and detail, form and submit |

Pages compose components; they do not reinterpret state or control the kernel directly. Shared composites do not fetch domain data by default. Domain components read state from application services and send intents to them. Report service outcomes rather than assuming success in the UI.

Document when each shared component is appropriate, its parts, inputs and actions, sizes and variants, applicable states (loading, empty, error, read-only, disabled), and keyboard and focus behavior. Examples must cover both themes, long text, and relevant exceptional states. Descriptions are optional; omit the description region when there is nothing useful to add.

## 6. Interaction rules

Use these rules for behavior that spans features. Subscription retry eligibility, routing, and configuration composition remain feature decisions; how states and recovery are presented follows this standard.

| Interaction | Rule |
| --- | --- |
| Primary action | Use one primary action at a time. Use a verb and object, such as “Save subscription” or “Apply configuration.” Use switches only for clear, immediately reversible binary settings. For multi-stage connection operations, show the user's intent separately from the confirmed outcome. |
| Settings, configuration, and drafts | Distinguish unsaved, saved but not applied, applying, effective, rejected, and unconfirmed states. Identify whether a change affects the current profile or all profiles before submission. Preserve inputs on rejection. Reconcile an unknown outcome before retrying; do not automatically resubmit. |
| Connection and recovery | Distinguish requested, in progress, traffic capture confirmed, connectivity verified, failed, and unknown. A running kernel, enabled switch, or confirmed capture alone does not prove connectivity. Put a useful next step next to a failure; a transient notification is not sufficient. |
| Dialogs, details, and return | Use a dialog for a focused decision and a detail panel for ordinary browsing. Name dialogs, contain focus while open, and restore it to the trigger on closing. Apply the same draft policy to Escape and Cancel. Preserve filters and selection on return to a list. |
| Feedback, confirmation, and availability | Distinguish loading, no data, no search results, failure, and unsupported states. Ask for confirmation when proceeding would discard edits, delete data, or materially affect networking; state the consequence. Explain why an action is unavailable, rather than describing Lite as a permission restriction. |
| Tables, keyboard, and focus | Make core tasks operable by keyboard and keep focus visible. Do not indicate selection with color alone. Make long addresses and identifiers available in full or copyable. Virtualization must preserve keyboard navigation. |

### 6.1 Choose a form container

| Scenario | Container |
| --- | --- |
| A few fields, with no need to refer to the page | Dialog, such as adding a subscription |
| Editing requires reference to the list or selected item | Side panel that preserves list context |
| Many fields, several groups, or multiple steps | Dedicated editor, such as a full proxy configuration form |

Do not put an entire create or edit form inside a page accordion that pushes a list or surrounding content out of the way. Collapse only optional advanced fields or help. Required fields, current validation errors, and Save and Cancel must not remain hidden. Automatically expand an advanced group when a field in it becomes required.

### 6.2 Keep the submit action visible

- Dialogs, side-panel editors, and dedicated editors share a bottom action bar. Keep primary and cancel actions in consistent positions. The form body scrolls independently; reserve space for the bar so it cannot cover content. At narrow widths, allow labels and actions to reflow. Do not move the primary action to the header, hide it in an overflow menu, or require scrolling to the end to find it.
- When saving and applying are distinct, label them separately and make the next step primary. While submitting, keep the bar and buttons in place, show progress, and prevent duplicate submissions. A disabled button alone must not be the only indication of correctable field errors.
- Use the same draft policy for Close, Back, and Escape; confirm only if changes would be lost. Indicate success explicitly. Re-enabling a button is not success feedback.

### 6.3 Put errors where they can be fixed

| Error | Placement and behavior |
| --- | --- |
| Invalid field | Place corrective guidance below the field and associate it with the input. On submit, expand the section containing the first invalid field, scroll the field and its error clear of the action bar, and focus the input. A summary near Submit may supplement these messages. |
| Save or apply failure | Keep persistent feedback near the bottom action bar, preserve input, and explain the next step. Put field-specific errors beside their fields. If the outcome is unknown, provide a way to check the current state rather than retrying automatically. |
| App-wide issue | Reserve the app header for issues affecting multiple pages or global operation. Do not send local errors to the header or rely on a transient notification alone. |

## 7. Lite, Pro, and observation widgets

- **Lite:** Subscription import, starting the proxy, and necessary recovery must always be reachable, outside removable widgets. Do not require routing knowledge or dashboard setup. Surface an advanced action in context when required instead of forcing a mode switch.
- **Pro:** Organize around observing, diagnosing, editing, and verifying. Reuse connection and configuration components. The modes vary in presentation and depth, not permission or runtime instance. Switching modes must not reconnect, discard drafts, or change the active profile.
- **Widget patterns:** Live traffic, active connections, and traffic capture status are representative widgets. Reuse metric summaries, trends, ranked lists, and status indicators. Omit a ranking if its data cannot support the claim. Ship a usable default dashboard arrangement and keep important actionable notices outside its configurable region.
- **Structure and editing:** Standardize the title, value or visual, necessary time and data scope, loading/stale/unavailable states, and optional detail entry. Compact widgets answer one question; wider variants can add a trend, distribution, or longer list. Preserve filters on entry to details. Provide a separate layout edit mode with add, remove, keyboard reorder, Save, Cancel, and Restore defaults.
- **Data and performance:** Use the same sources and units for summaries and details. Do not poll independently from each widget. Pause unnecessary hidden updates and bound retained samples and rendering work. Distinguish missing data (`—`) from measured zero, mark stale values with their time, and show gaps for missing samples. Do not present partial counts as totals. Rates show units and direction, latency uses milliseconds, trends show a time window, and binary units must not be labeled as decimal.
- **Local ownership:** Subscriptions, imported proxy files, and upstream plugins must not control the UI, mode, or widget layout. Rearranging widgets must not change network configuration or reconnect.

## 8. Acceptance

Use shared components and design tokens for these examples. Do not create page-specific versions to pass review.

| Scenario | Pass criteria |
| --- | --- |
| First connection | Import a subscription and start the proxy in Lite. The current profile and confirmed state are understandable without moving to Pro. |
| Failure and recovery | Show the failure at the initiating control with a usable recovery path. A repeat click or unconfirmed result must not appear as success; do not automatically resubmit. |
| Edit and apply configuration | Make profile scope and saved versus effective states clear. On rejection, preserve inputs and show the reason and next step by the action bar. |
| Submit a long form | In a narrow window, the primary action remains visible at the start, middle, and end without obscuring fields. Keyboard submission of missing or invalid input brings the first invalid field and its error into view above the action bar and moves focus to the field. A failed save preserves the draft. |
| Switch between equivalent pages | Compare Nodes, Subscriptions, and Connections side by side, then switch between them. Check title alignment, stable filter and action placement, and consistent narrow-window detail behavior. |
| Operate without explanatory clutter | Without redundant descriptions, the current state, main action, and grouping are clear. Essential warnings and corrective guidance are available without hovering. |

Check light and dark themes at both review sizes. Complete core flows with the keyboard, and change the operating-system theme with an open draft. Review long text, both supported UI languages, missing and stale data, reduced motion, and error states. Measure contrast in the rendered UI. Report component previews, interaction checks, and installed network behavior as separate evidence.
