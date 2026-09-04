/** A portaled menu owns its keyboard interaction before its parent modal. */
export function isNestedOverlayEvent(event: KeyboardEvent): boolean {
  if (event.defaultPrevented) return true;
  return event.composedPath().some((target) => target instanceof Element && target.matches(
    '[data-slot="select-content"], [data-slot="select-trigger"][aria-expanded="true"], [data-slot="popover-content"], [role="listbox"], [role="menu"]',
  ));
}
