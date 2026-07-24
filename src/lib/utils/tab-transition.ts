export type TabTransitionDirection = -1 | 1;

export function getTabTransitionDirection(
  tabOrder: readonly string[],
  previousTab: string,
  nextTab: string,
): TabTransitionDirection {
  const previousIndex = tabOrder.indexOf(previousTab);
  const nextIndex = tabOrder.indexOf(nextTab);

  if (previousIndex < 0 || nextIndex < 0 || previousIndex === nextIndex) {
    return 1;
  }

  return nextIndex < previousIndex ? -1 : 1;
}
