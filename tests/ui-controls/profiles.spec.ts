import { test, expect } from '@playwright/test';

test('profile toolbar keeps desktop actions on one row at the normal compact window width', async ({ page }) => {
  await page.setViewportSize({ width: 625, height: 650 });
  await page.goto('/?panel=profiles');
  await expect(page.getByText('代理配置', { exact: true })).toBeVisible();

  const title = page.locator('.toolbar-top .title-block');
  const actions = page.locator('.toolbar-top .current-block');
  const [titleBox, actionsBox] = await Promise.all([title.boundingBox(), actions.boundingBox()]);
  expect(titleBox).not.toBeNull();
  expect(actionsBox).not.toBeNull();
  expect(Math.abs(actionsBox!.y - titleBox!.y)).toBeLessThan(10);
  expect(actionsBox!.x).toBeGreaterThan(titleBox!.x + titleBox!.width);
  await expect(page.locator('.profiles-root')).not.toHaveCSS('overflow-x', 'scroll');
});

for (const width of [900, 625, 375]) {
  test(`profile search reserves icon space at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 500 });
    await page.goto('/?panel=profiles');
    const input = page.getByRole('textbox', { name: '搜索配置' });
    await expect(input).toBeVisible();
    const icon = page.locator('.toolbar-search .search-icon');
    const bounds = await input.boundingBox();
    const iconBounds = await icon.boundingBox();
    const padding = await input.evaluate(el => parseFloat(getComputedStyle(el).paddingLeft));
    expect(bounds!.x + padding).toBeGreaterThan(iconBounds!.x + iconBounds!.width + 3);
    await input.fill('狗梯');
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(width);
  });
}
