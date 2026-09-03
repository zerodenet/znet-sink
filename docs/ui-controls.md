# Client control contract

The same control surface is used on Windows, macOS and Linux, in both UI modes
and both themes. Page components own layout and content, not platform-specific
form chrome.

- Actions use `ui/button` with `default`, `outline`, `ghost`, `destructive` or
  `link` semantics. The shared sizes are compact, regular and comfortable.
- Text and numeric inputs use `ui/input`; multiline and JSON editors use
  `ui/textarea`. Preserve `bind:ref` when selection, focus or scroll code needs
  the real DOM element. Numeric inputs retain validation and keyboard stepping;
  OS-specific spin buttons are hidden.
- Selectors use `ui/select` (or `field-select.svelte` for simple option lists).
  Never add a native `<select>` to a page. Values are strings at this boundary:
  explicitly convert numbers before passing them to existing configuration APIs.
- Checkbox/radio inputs use `ui/choice`, which shares theme styling while keeping
  native label, Space and radio-group arrow-key behavior. Toggles use `ui/switch`.
- Tabs use `AppTabs`; mutually exclusive view/mode choices use
  `AppSegmentedControl`, not hand-written active classes.
- Cards, navigation rows, window controls and backdrop hit targets may retain a
  native button marked `data-slot="surface-button"`. These are layout-specific
  surfaces, not an alternative action-button design. Shared focus, disabled and
  appearance rules still apply.

`src/app.css` owns theme/size/layer tokens; `ui/controls.css` owns shared field
appearance and interaction states. Normal fields use a 30 px height and 7 px
radius; dense controls use the existing 26 px size. Code editors may override
font metrics to match their line-number gutter, but not add native styling.

Menus portal outside scroll containers, sit above dialogs and are bounded by
available viewport height. Custom modal keyboard handlers defer to nested menus
so the first Escape closes the menu, not the entire dialog. Do not restore
per-page z-index overrides or focus rings for standard controls.

## Verification

- `pnpm test:ui-controls`: compile all Svelte components and scan all business
  components for native form controls and hand-written tab/radio controls.
- `pnpm test:ui-browser`: Chromium/WebKit UI-only fixture tests, including the
  real Rules page. Configuration calls are replaced with in-memory fixtures;
  no kernel, DNS, TUN or system proxy is started or modified.
- GitHub-hosted UI checks retain light/dark screenshots and failure traces.
  Browser-engine tests do not replace macOS WKWebView or Windows WebView2
  packaged-client acceptance on the user's test machine.
