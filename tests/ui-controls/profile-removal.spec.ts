import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
interface Call { kind: string; targetId?: string; input?: { targetId: string; guard: string; action: { kind: string; replacementId?: string } } }
const ack = '我确认删除上述固定配置，并接受所列关联及连接影响';
async function calls(page: Page): Promise<Call[]> {
  return page.locator('#profile-removal-calls').evaluate(element => JSON.parse(element.textContent ?? '[]'));
}
async function open(page: Page, mode: string) {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.goto(`/?panel=profile-removal&removal=${mode}`);
  await expect(page.getByRole('dialog', { name: '删除配置' })).toBeVisible();
  await expect(page.getByRole('dialog')).toContainText('保留的关联订阅');
}
async function chooseReplacement(page: Page) {
  await page.getByRole('radio', { name: '明确选择替代配置', exact: true }).check();
  await page.getByRole('button', { name: '替代配置', exact: true }).click();
  await page.getByRole('option', { name: '明确指定的替代配置', exact: true }).click();
}
test('source and links are previewed, no first replacement is chosen, and unchecked confirmation cannot delete', async ({ page }) => {
  await open(page, 'replace');
  const submit = page.getByRole('button', { name: '确认删除', exact: true });
  await expect(page.getByRole('radio', { name: '明确选择替代配置', exact: true })).not.toBeChecked();
  await expect(page.getByRole('radio', { name: '停止运行并删除，不自动启用其他配置', exact: true })).not.toBeChecked();
  await expect(submit).toBeDisabled();
  await expect(page.getByRole('checkbox', { name: ack, exact: true })).toBeDisabled();
  await chooseReplacement(page);
  await expect(page.getByRole('button', { name: '替代配置', exact: true })).toContainText('明确指定的替代配置');
  await expect(submit).toBeDisabled();
  expect((await calls(page)).filter(call => call.kind === 'commit')).toHaveLength(0);
  expect(await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth)).toBe(0);
});
test('explicit chosen replacement commits exactly the fixed profile/version once after impact acknowledgement', async ({ page }) => {
  await open(page, 'replace'); await chooseReplacement(page);
  await page.getByRole('checkbox', { name: ack, exact: true }).check();
  await page.getByRole('button', { name: '确认删除', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const commits = (await calls(page)).filter(call => call.kind === 'commit');
  expect(commits).toHaveLength(1);
  expect(commits[0].input).toEqual({ targetId: 'profile-current', guard: 'fixture-fixed-source-version-and-relationship-proof', action: { kind: 'replace', replacementId: 'replacement-chosen' } });
  await expect(page.locator('#profile-removal-outcomes')).toContainText('"replacementId":"replacement-chosen"');
});
test('no-alternative active removal requires explicit stop and acknowledgement, then reports actual stop receipt', async ({ page }) => {
  await open(page, 'stop');
  await page.getByRole('radio', { name: '停止运行并删除，不自动启用其他配置', exact: true }).check();
  await expect(page.getByRole('dialog')).toContainText('先实际停止运行并释放本客户端接管');
  await expect(page.getByRole('button', { name: '确认删除', exact: true })).toBeDisabled();
  await page.getByRole('checkbox', { name: ack, exact: true }).check();
  await page.getByRole('button', { name: '确认删除', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect((await calls(page)).filter(call => call.kind === 'commit')[0].input?.action).toEqual({ kind: 'stop' });
  await expect(page.locator('#profile-removal-outcomes')).toContainText('"runtimeStopped":true');
});
test('native active unknown blocks deletion while inactive local deletion needs no VPN stop', async ({ page }) => {
  await open(page, 'native-active');
  await expect(page.getByRole('dialog')).toContainText('平台 VPN/运行服务状态尚未确认');
  await expect(page.getByRole('button', { name: '确认删除', exact: true })).toBeDisabled();
  await expect(page.getByRole('checkbox', { name: ack, exact: true })).toHaveCount(0);
  expect((await calls(page)).filter(call => call.kind === 'commit')).toHaveLength(0);
  await open(page, 'native-inactive');
  await expect(page.getByRole('dialog')).toContainText('平台或运行状态未确认');
  await page.getByRole('checkbox', { name: ack, exact: true }).check();
  await page.getByRole('button', { name: '确认删除', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const commit = (await calls(page)).find(call => call.kind === 'commit');
  expect(commit?.input).toEqual({ targetId: 'profile-inactive', guard: 'fixture-fixed-source-version-and-relationship-proof', action: { kind: 'inactive' } });
  await expect(page.locator('#profile-removal-outcomes')).toContainText('"runtimeStopped":false');
});
test('unknown outcome only checks original operation and reopening persistent pending state cannot replay', async ({ page }) => {
  await open(page, 'unknown'); await chooseReplacement(page);
  await page.getByRole('checkbox', { name: ack, exact: true }).check();
  await page.getByRole('button', { name: '确认删除', exact: true }).click();
  await expect(page.getByRole('button', { name: '核对原操作', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '确认删除', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '核对原操作', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('测试运行结果仍待核对');
  expect((await calls(page)).filter(call => call.kind === 'commit')).toHaveLength(1);
  expect((await calls(page)).filter(call => call.kind === 'reconcile')).toEqual([{ kind: 'reconcile', targetId: 'profile-current' }]);
  await expect(page.locator('#profile-removal-outcomes')).toHaveText('[]');
  await page.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByRole('button', { name: '重新打开测试删除对话框', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('此前删除协调结果未确认');
  await expect(page.getByRole('button', { name: '核对原操作', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: '确认删除', exact: true })).toHaveCount(0);
  expect((await calls(page)).filter(call => call.kind === 'commit')).toHaveLength(1);
});
