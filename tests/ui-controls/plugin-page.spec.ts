import { createServer } from 'node:http';
import { test, expect, type Page } from '@playwright/test';
const frame = (page: Page) =>
  page.frameLocator('iframe[title="Signed fixture"]');
async function counters(page: Page) {
  return JSON.parse(await page.getByLabel('Host counters').innerText());
}
async function open(page: Page) {
  await page.goto('/?panel=plugin-page');
  await expect(
    frame(page).getByRole('heading', { name: 'Signed fixture page' }),
  ).toBeVisible();
  await expect.poll(async () => (await counters(page)).documents).toBe(1);
}
test('explicit unknown component IDs reject without invoking the first component; SDK browser receipt is not opened twice', async ({
  page,
}) => {
  await open(page);
  await frame(page).getByRole('button', { name: 'Unknown component' }).click();
  await expect(frame(page).locator('#result')).toContainText(
    'invalid_request:',
  );
  expect((await counters(page)).invokes).toBe(0);
  await frame(page).getByRole('button', { name: 'Current operation' }).click();
  await expect(frame(page).locator('#result')).toContainText('document-1');
  await frame(page).getByRole('button', { name: 'Open browser' }).click();
  await expect(frame(page).locator('#result')).toHaveText(
    '{"state":"accepted"}',
  );
  expect((await counters(page)).sdkCalls).toBe(1);
  expect(page.context().pages()).toHaveLength(1);
});
for (const change of [
  'New installation',
  'New registration',
  'New revision',
  'Disable component',
]) {
  test(`a delayed result cannot reach the replacement frame after ${change}`, async ({
    page,
  }) => {
    await open(page);
    await frame(page)
      .getByRole('button', { name: 'Delayed operation' })
      .click();
    await expect.poll(async () => (await counters(page)).invokes).toBe(1);
    await page.getByRole('button', { name: change, exact: true }).click();
    await expect.poll(async () => (await counters(page)).documents).toBe(2);
    await expect(frame(page).getByRole('heading')).toBeVisible();
    await page.getByRole('button', { name: 'Resolve old operation' }).click();
    await expect(frame(page).locator('#result')).toHaveText('ready');
    expect((await counters(page)).closes).toBe(1);
    await frame(page)
      .getByRole('button', { name: 'Current operation' })
      .click();
    await expect(frame(page).locator('#result')).toContainText(
      change === 'Disable component' ? 'permission_denied:' : 'document-2',
    );
  });
}
test('file chooser cannot use a revoked page grant after selection; no stale native file operation is sent', async ({
  page,
}) => {
  await open(page);
  await frame(page).getByRole('button', { name: 'Choose file' }).click();
  await expect.poll(async () => (await counters(page)).pickers).toBe(1);
  await page.getByRole('button', { name: 'Revoke silently' }).click();
  await page.getByRole('button', { name: 'Finish picker' }).click();
  await expect(page.getByRole('alert')).toContainText('原通道已关闭');
  expect((await counters(page)).sdkCalls).toBe(0);
  expect((await counters(page)).preflights).toBe(1);
  expect(await page.textContent('body')).not.toContain('SECRET');
});
test('file cancellation preserves the page, actual picker path is host-bound and oversized result is safely rejected', async ({
  page,
}) => {
  await open(page);
  await frame(page).getByRole('button', { name: 'Choose file' }).click();
  await expect.poll(async () => (await counters(page)).pickers).toBe(1);
  await page.getByRole('button', { name: 'Cancel picker' }).click();
  await expect(frame(page).locator('#result')).toContainText('cancelled:');
  expect((await counters(page)).sdkCalls).toBe(0);
  await frame(page).getByRole('button', { name: 'Choose file' }).click();
  await expect.poll(async () => (await counters(page)).pickers).toBe(2);
  await page.getByRole('button', { name: 'Finish picker' }).click();
  await expect(frame(page).locator('#result')).toContainText('/host/chosen');
  await frame(page).getByRole('button', { name: 'Oversized result' }).click();
  await expect(frame(page).locator('#result')).toContainText(
    'budget_exceeded:',
  );
});
test('background closes the old channel before authoritative foreground snapshot and new document', async ({
  page,
}) => {
  await open(page);
  await frame(page).getByRole('button', { name: 'Delayed operation' }).click();
  await expect.poll(async () => (await counters(page)).invokes).toBe(1);
  await page.getByRole('button', { name: 'Background', exact: true }).click();
  await expect(page.locator('iframe')).toHaveCount(0);
  expect((await counters(page)).closes).toBe(1);
  await page.getByRole('button', { name: 'Foreground', exact: true }).click();
  await expect(frame(page).getByRole('heading')).toBeVisible();
  await expect.poll(async () => (await counters(page)).snapshots).toBe(1);
  expect((await counters(page)).documents).toBe(2);
  await page.getByRole('button', { name: 'Resolve old operation' }).click();
  await expect(frame(page).locator('#result')).toHaveText('ready');
});
test('parent CSP blocks external iframe navigation and retires the old channel when the guest leaves', async ({
  page,
}) => {
  let requests = 0;
  page.on('request', (request) => {
    if (request.url().includes('navigation.invalid')) requests++;
  });
  await open(page);
  await frame(page)
    .getByRole('button', { name: 'Navigate', exact: true })
    .click();
  await expect(page.getByRole('alert')).toContainText(/原通道关闭|发生导航/);
  expect(requests).toBe(0);
  expect((await counters(page)).closes).toBe(1);
  await expect(page.locator('iframe')).toHaveCount(0);
});

for (const mode of ['pre-head-script', 'pre-head-image', 'comment-fake-head']) {
  test(`trusted CSP precedes signed guest ${mode} bytes and SDK remains usable`, async ({
    page,
  }) => {
    const received: string[] = [];
    const probe = createServer((request, response) => {
      received.push(request.url ?? '');
      response.writeHead(200, {
        'Content-Type': 'image/png',
        'Access-Control-Allow-Origin': '*',
      });
      response.end();
    });
    await new Promise<void>((resolve) => probe.listen(0, '127.0.0.1', resolve));
    const address = probe.address();
    if (!address || typeof address === 'string')
      throw new Error('probe address unavailable');
    const origin = `http://127.0.0.1:${address.port}`;
    const attempts: Array<{ url: string; failure?: string }> = [];
    page.on('requestfailed', (request) => {
      if (request.url().startsWith(origin))
        attempts.push({
          url: request.url(),
          failure: request.failure()?.errorText,
        });
    });
    try {
      await page.goto(
        `/?panel=plugin-page&document-mode=${mode}&probe-origin=${encodeURIComponent(origin)}`,
      );
      await expect(
        frame(page).getByRole('heading', { name: 'Signed fixture page' }),
      ).toBeVisible();
      await expect
        .poll(() =>
          frame(page)
            .locator('body')
            .evaluate(() => !!(window as any).__networkAttempted),
        )
        .toBe(true);
      await expect
        .poll(() =>
          frame(page)
            .locator('body')
            .evaluate(() => ((window as any).__cspViolations ?? []).length),
        )
        .toBeGreaterThan(0);
      await frame(page)
        .getByRole('button', { name: 'Current operation' })
        .click();
      await expect(frame(page).locator('#result')).toContainText('document-1');
      expect(received).toEqual([]);
      expect(
        attempts.every((request) => /csp|blocked/i.test(request.failure ?? '')),
      ).toBe(true);
      expect((await counters(page)).invokes).toBe(1);
    } finally {
      await new Promise<void>((resolve, reject) =>
        probe.close((error) => (error ? reject(error) : resolve())),
      );
    }
  });
}
