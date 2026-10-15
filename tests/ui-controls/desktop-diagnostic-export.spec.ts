import { expect, test } from '@playwright/test';

const open = async (page: import('@playwright/test').Page, mode = '') => {
  await page.goto(`/?panel=desktop-diagnostic-export&mode=${mode}`);
  await page.getByRole('button', { name: '导出诊断材料', exact: true }).click();
  return page.getByRole('dialog').filter({ hasText: '预览桌面诊断材料' });
};

test('desktop export previews actual fixed scope and omissions before explicit delivery', async ({ page }) => {
  const dialog = await open(page);
  await expect(dialog.getByLabel('固定桌面诊断快照')).toContainText('profile-original');
  await expect(dialog).toContainText('最高2MiB');
  await expect(dialog).toContainText('跨配置、实例的有界历史，任务限当前作用域');
  await expect(page.getByLabel('诊断交付计划')).toHaveText('[]');
  await dialog.getByText('省略范围与原因', { exact: true }).click();
  await expect(dialog).toContainText('自由字段没有可信分类，已省略');
  await dialog.getByRole('button', { name: '确认保存原快照' }).click();
  await expect(page.getByLabel('诊断交付计划')).toHaveText('["fixed-preview-1"]');
  await expect(page.getByLabel('已确认保存次数')).toHaveText('1');
  await expect(dialog.getByRole('button', { name: '确认保存原快照' })).toBeDisabled();
});

test('cancel keeps original storage untouched and never delivers a plan', async ({ page }) => {
  const dialog = await open(page);
  await dialog.getByRole('button', { name: '返回', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect(page.getByLabel('已放弃计划')).toHaveText('["fixed-preview-1"]');
  await expect(page.getByLabel('诊断交付计划')).toHaveText('[]');
  await page.getByRole('button', { name: '导出诊断材料', exact: true }).click();
  await expect(page.getByLabel('诊断预览次数')).toHaveText('2');
  await page.getByRole('button', { name: '确认保存原快照' }).click();
  await expect(page.getByLabel('诊断交付计划')).toHaveText('["fixed-preview-2"]');
});

test('unknown IO retires plan, displays no untrusted error and requires new explicit preview', async ({ page }) => {
  const dialog = await open(page, 'failure');
  await dialog.getByRole('button', { name: '确认保存原快照' }).click();
  await expect(dialog).toContainText('诊断导出未确认');
  await expect(dialog).not.toContainText('untrusted-error-canary');
  await expect(dialog.getByRole('button', { name: '确认保存原快照' })).toBeDisabled();
  await expect(page.getByLabel('诊断交付计划')).toHaveText('["fixed-preview-1"]');
  await expect(page.getByLabel('已确认保存次数')).toHaveText('0');
});

test('pending write blocks duplicate confirmation, cancel and Escape without replay', async ({ page }) => {
  const dialog = await open(page, 'pending');
  await dialog.getByRole('button', { name: '确认保存原快照' }).click();
  await expect(dialog.getByRole('button', { name: '等待保存回执…' })).toBeDisabled();
  await expect(dialog.getByRole('button', { name: '返回', exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeVisible();
  await page.evaluate(() => (window as unknown as { releaseDesktopDiagnosticExport(): void }).releaseDesktopDiagnosticExport());
  await expect(page.getByLabel('已确认保存次数')).toHaveText('1');
  await expect(page.getByLabel('诊断交付计划')).toHaveText('["fixed-preview-1"]');
});

test('projection failure cannot fall back to raw files or claim success', async ({ page }) => {
  const dialog = await open(page, 'preview-fail');
  await expect(dialog).toContainText('不会改用原始日志');
  await expect(dialog).not.toContainText('untrusted-error-canary');
  await expect(dialog.getByRole('button', { name: '确认保存原快照' })).toBeDisabled();
  await expect(page.getByLabel('诊断交付计划')).toHaveText('[]');
});


test('closed pending preview releases its late plan and never exposes it for delivery', async ({ page }) => {
  const dialog = await open(page, 'preview-pending');
  await expect(dialog).toContainText('正在准备实际快照');
  await dialog.getByRole('button', { name: '返回', exact: true }).click();
  await page.evaluate(() => (window as unknown as { releaseDesktopDiagnosticExport(): void }).releaseDesktopDiagnosticExport());
  await expect(page.getByLabel('已放弃计划')).toHaveText('["fixed-preview-1"]');
  await expect(page.getByLabel('诊断交付计划')).toHaveText('[]');
});


test('expired or changed-scope plan refusal remains distinct from unknown file delivery', async ({ page }) => {
  const dialog = await open(page, 'rejected');
  await dialog.getByRole('button', { name: '确认保存原快照' }).click();
  await expect(dialog).toContainText('预览已失效或请求被拒绝');
  await expect(dialog).toContainText('未发起新的文件写入');
  await expect(dialog).not.toContainText('untrusted-error-canary');
  await expect(dialog.getByRole('button', { name: '确认保存原快照' })).toBeDisabled();
  await expect(page.getByLabel('已确认保存次数')).toHaveText('0');
});


test('actual desktop error recovery opens About for explicit preview without preparing or delivering any plan', async ({ page }) => {
  await page.goto('/?panel=desktop-diagnostic-export');
  await page.getByRole('region', { name: '桌面诊断出口实际组件测试' }).getByRole('button', { name: '预览诊断导出', exact: true }).click();
  await expect(page.getByLabel('恢复入口导航')).toHaveText('["about"]');
  await expect(page.getByLabel('诊断预览次数')).toHaveText('0');
  await expect(page.getByLabel('诊断交付计划')).toHaveText('[]');
  await expect(page.getByRole('dialog')).toHaveCount(0);
});
