import { test, expect, type Page } from '@playwright/test';

async function readToolbarLayout(page: Page) {
  return page.evaluate(() => {
    const rect = (selector: string) => {
      const value = (document.querySelector(selector) as HTMLElement).getBoundingClientRect();
      return {left: value.left, right: value.right, top: value.top, bottom: value.bottom, width: value.width, height: value.height};
    };
    const toolbar = document.querySelector('.node-toolbar') as HTMLElement;
    const actions = document.querySelector('.toolbar-actions') as HTMLElement;
    return {
      toolbar: rect('.node-toolbar'),
      identity: rect('.toolbar-left'),
      controls: rect('.toolbar-right'),
      search: rect('.search-wrap'),
      actions: rect('.toolbar-actions'),
      toolbarOverflow: toolbar.scrollWidth - toolbar.clientWidth,
      actionOverflow: actions.scrollWidth - actions.clientWidth,
      documentOverflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    };
  });
}

test('node toolbar keeps its controls inside the panel at compact desktop widths', async ({ page }) => {
  for (const width of [902, 760, 620]) {
    await page.setViewportSize({ width, height: 750 });
    await page.goto('/?panel=nodes&layout=compact');
    await expect(page.locator('.node-toolbar')).toBeVisible();
    await expect(page.getByRole('button', {name: '测试全部节点延迟'})).toBeVisible();
    await expect(page.getByRole('button', {name: '测试全部节点延迟'})).toHaveText('');

    const layout = await readToolbarLayout(page);
    expect(layout.toolbarOverflow).toBeLessThanOrEqual(0);
    expect(layout.actionOverflow).toBeLessThanOrEqual(0);
    expect(layout.documentOverflow).toBeLessThanOrEqual(0);
    expect(layout.controls.left).toBeGreaterThanOrEqual(layout.toolbar.left);
    expect(layout.controls.right).toBeLessThanOrEqual(layout.toolbar.right);
    expect(layout.actions.right).toBeLessThanOrEqual(layout.toolbar.right);
    expect(layout.search.width).toBeGreaterThanOrEqual(100);
  }
});

test('large probe progress has a fixed slot and does not squeeze the toolbar', async ({ page }) => {
  for (const width of [902, 760, 620]) {
    await page.setViewportSize({width, height: 750});
    await page.goto('/?panel=nodes&layout=compact&probe=large');

    const stop = page.getByRole('button', {name: '停止全部节点测速'});
    await expect(stop).toBeVisible();
    await expect(stop).toHaveText('');
    await expect(page.getByRole('status', {name: '节点测速进度 987/1234'})).toContainText('987/1234');

    const layout = await readToolbarLayout(page);
    const progressWidth = await page.locator('.probe-progress').evaluate((element) => element.getBoundingClientRect().width);
    expect(progressWidth).toBe(96);
    expect(layout.toolbarOverflow).toBeLessThanOrEqual(0);
    expect(layout.actionOverflow).toBeLessThanOrEqual(0);
    expect(layout.actions.right).toBeLessThanOrEqual(layout.toolbar.right);
  }
});

test('compact card density is not widened to solve toolbar pressure', async ({ page }) => {
  await page.setViewportSize({width: 886, height: 750});
  await page.goto('/?panel=nodes&layout=compact');
  await expect(page.locator('.node-grid')).toBeVisible();

  const layout = await page.evaluate(() => {
    const grid = document.querySelector('.node-grid') as HTMLElement;
    const sidebar = document.querySelector('.group-sidebar') as HTMLElement;
    return {
      columns: getComputedStyle(grid).gridTemplateColumns.split(' ').length,
      sidebarWidth: sidebar.getBoundingClientRect().width,
    };
  });
  expect(layout.columns).toBeGreaterThanOrEqual(3);
  expect(layout.sidebarWidth).toBe(168);
});
