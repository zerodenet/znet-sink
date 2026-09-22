import { test, expect } from '@playwright/test';

test('node cards use readable columns at compact desktop widths', async ({ page }) => {
  for (const [width, expectedColumns, minimumSidebar] of [
    [886, 2, 210],
    [1200, 3, 210],
    [620, 1, 150],
  ]) {
    await page.setViewportSize({ width, height: 750 });
    await page.goto('/?panel=nodes&layout=compact');
    await expect(page.locator('.node-grid')).toBeVisible();

    const layout = await page.evaluate(() => {
      const root = document.querySelector('.nodes-root') as HTMLElement;
      const sidebar = document.querySelector('.group-sidebar') as HTMLElement;
      const panel = document.querySelector('.node-panel') as HTMLElement;
      const grid = document.querySelector('.node-grid') as HTMLElement;
      const card = document.querySelector('.grid-card') as HTMLElement;
      const selected = document.querySelector('.group-selected-name') as HTMLElement;
      return {
        columns: getComputedStyle(grid).gridTemplateColumns.split(' ').length,
        sidebarWidth: sidebar.getBoundingClientRect().width,
        rootRight: root.getBoundingClientRect().right,
        cardRight: card.getBoundingClientRect().right,
        panelRight: panel.getBoundingClientRect().right,
        documentWidth: document.documentElement.scrollWidth,
        selectedFits: selected.scrollWidth <= selected.clientWidth,
      };
    });
    expect(layout.columns).toBe(expectedColumns);
    expect(layout.sidebarWidth).toBeGreaterThanOrEqual(minimumSidebar);
    expect(layout.rootRight).toBeLessThanOrEqual(width);
    expect(layout.cardRight).toBeLessThanOrEqual(layout.panelRight);
    expect(layout.documentWidth).toBeLessThanOrEqual(width);
    if (width === 886) expect(layout.selectedFits).toBe(true);
  }
});
