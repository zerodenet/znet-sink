import { expect, test } from '@playwright/test';

test('unknown inspection fixes source and preserves uncertainty without synchronize, cache reset or unsafe revision rounding', async ({
  page,
}) => {
  await page.addInitScript(() => {
    const win = window as unknown as {
      inspections: unknown[];
      mutations: unknown[];
    };
    win.inspections = [];
    win.mutations = [];
    window.addEventListener('fixture-subscription-inspection', (e) =>
      win.inspections.push((e as CustomEvent).detail),
    );
    for (const event of [
      'fixture-subscription-manual-sync',
      'fixture-subscription-cache-reset',
    ])
      window.addEventListener(event, (e) =>
        win.mutations.push((e as CustomEvent).detail),
      );
  });
  await page.setViewportSize({ width: 320, height: 480 });
  await page.goto('/?panel=subscriptions&mode=quota-facts');
  await page.getByRole('button', { name: '核对未知结果', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('原运行提交待核对');
  await expect
    .poll(() =>
      page.evaluate(
        () => (window as unknown as { inspections: unknown[] }).inspections,
      ),
    )
    .toEqual([]);
  await page.getByRole('button', { name: '读取核对状态', exact: true }).click();
  await expect(dialog).toContainText('当前身份可读；不代表原操作生效');
  await expect(dialog).toContainText('18446744073709551615');
  await expect(dialog).toContainText('没有持久记录');
  await expect(dialog).toContainText('结果仍未确认');
  await expect(
    dialog.getByRole('button', { name: '完成本地保存', exact: true }),
  ).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(
        () => (window as unknown as { inspections: unknown[] }).inspections,
      ),
    )
    .toEqual([{ id: 'uncertain', version: 1, profileId: 'work' }]);
  expect(
    await page.evaluate(
      () => (window as unknown as { mutations: unknown[] }).mutations,
    ),
  ).toEqual([]);
  const button = await page
    .getByRole('button', { name: '重新核对', exact: true })
    .boundingBox();
  expect(button).not.toBeNull();
  expect(button!.height).toBeGreaterThanOrEqual(44);
  expect(button!.y + button!.height).toBeLessThanOrEqual(480);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await expect(
    page
      .locator('.list-row')
      .filter({ has: page.getByText('原运行提交待核对', { exact: true }) }),
  ).toContainText('结果待核对');
});

test('closing read-only inspection permits return while late reply cannot recreate the panel or clear source state', async ({
  page,
}) => {
  await page.goto('/?panel=subscriptions&mode=quota-facts&inspection=late');
  await page.getByRole('button', { name: '核对未知结果', exact: true }).click();
  await page.getByRole('button', { name: '读取核对状态', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('正在读取');
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await page.evaluate(() =>
    window.dispatchEvent(new Event('fixture-release-inspection')),
  );
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(
    page
      .locator('.list-row')
      .filter({ has: page.getByText('原运行提交待核对', { exact: true }) }),
  ).toContainText('结果待核对');
});

test('managed subscription card shows aggregate quota, ratio, expiry and ownership', async ({
  page,
}) => {
  await page.goto('/?panel=subscriptions&mode=managed-usage');

  await expect(
    page.getByText('狗梯 / Developer Licenses', { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText('由 Connect 托管', { exact: true }),
  ).toBeVisible();
  await expect(page.getByText('来源: 狗梯', { exact: true })).toBeVisible();
  await expect(page.getByText(/已用 375 B \/ 总量 1000 B/)).toBeVisible();
  await expect(page.getByText('37.5%', { exact: true })).toBeVisible();
  await expect(page.getByText(/到期:/)).toBeVisible();
  await expect(page.getByText(/剩 \d+ 天/)).toBeVisible();
  await expect(page.getByRole('button', { name: '前往来源插件' })).toBeEnabled();
});

test('mounted subscription page refreshes the sync time on background completion', async ({
  page,
}) => {
  await page.goto('/?panel=subscriptions&mode=managed-usage');
  const syncTime = page.getByText(/^最近成功检查:/);
  await expect(syncTime).toBeVisible();
  const before = await syncTime.textContent();
  await page.evaluate(() => {
    window.dispatchEvent(
      new CustomEvent('fixture-subscription-synced', { detail: 1790568600000 }),
    );
    window.dispatchEvent(
      new CustomEvent('subscriptions:updated', {
        detail: { subscriptionId: 'managed-connect' },
      }),
    );
  });
  await expect(syncTime).not.toHaveText(before!);
  await expect(page.getByText(/已用 375 B \/ 总量 1000 B/)).toBeVisible();
});

test('320px subscription quota stays unknown for missing and unsafe usage, while real zero stays zero', async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 800 });
  await page.goto('/?panel=mobile-configuration&mode=quota-facts');
  await page.getByRole('radio', { name: '订阅', exact: true }).click();
  const row = (name: string) =>
    page
      .locator('.list-row')
      .filter({ has: page.getByText(name, { exact: true }) });
  for (const name of [
    '额度已提供但用量未知',
    '只有上传已采样',
    '数值无法确认',
  ]) {
    const card = row(name);
    await expect(card).toContainText('来源已用 — / 总量 100 B');
    await expect(card.locator('.traffic-bar-track')).toHaveCount(0);
    await expect(card.locator('.traffic-percent')).toHaveCount(0);
  }
  await expect(row('只有上传已采样')).toContainText('↑10 B ↓—');
  await expect(row('明确的零用量').locator('.traffic-percent')).toHaveText(
    '0.0%',
  );
  await expect(row('数值无法确认')).toContainText('来源数值无法确认');
  await expect(page.getByText('private-canary', { exact: false })).toHaveCount(
    0,
  );
  await expect(row('自动暂停仍可手动')).toContainText('自动同步已暂停');
  await expect(
    row('自动暂停仍可手动').getByRole('button', {
      name: '同步订阅',
      exact: true,
    }),
  ).toBeEnabled();
  await expect(row('额度已提供但用量未知')).toContainText('额度采样时间未提供');
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

test('explicit manual synchronization remains available when automatic participation is paused', async ({
  page,
}) => {
  await page.addInitScript(() => {
    (window as unknown as { manualSyncIds: string[] }).manualSyncIds = [];
    window.addEventListener('fixture-subscription-manual-sync', (event) =>
      (window as unknown as { manualSyncIds: string[] }).manualSyncIds.push(
        (event as CustomEvent<string>).detail,
      ),
    );
  });
  await page.goto('/?panel=subscriptions&mode=quota-facts');
  const paused = page
    .locator('.list-row')
    .filter({ has: page.getByText('自动暂停仍可手动', { exact: true }) });
  await paused.getByRole('button', { name: '同步订阅', exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () => (window as unknown as { manualSyncIds: string[] }).manualSyncIds,
      ),
    )
    .toEqual(['paused']);
  await expect(
    paused.getByRole('button', { name: '同步订阅', exact: true }),
  ).toBeEnabled();
});

test('cache recovery confirms only original source metadata, without synchronization or clearing unknown results', async ({
  page,
}) => {
  await page.addInitScript(() => {
    (
      window as unknown as { cacheResets: unknown[]; manualSyncIds: string[] }
    ).cacheResets = [];
    (window as unknown as { manualSyncIds: string[] }).manualSyncIds = [];
    window.addEventListener('fixture-subscription-cache-reset', (event) =>
      (window as unknown as { cacheResets: unknown[] }).cacheResets.push(
        (event as CustomEvent).detail,
      ),
    );
    window.addEventListener('fixture-subscription-manual-sync', (event) =>
      (window as unknown as { manualSyncIds: string[] }).manualSyncIds.push(
        (event as CustomEvent).detail,
      ),
    );
  });
  await page.goto('/?panel=subscriptions&mode=quota-facts');
  await page
    .getByRole('button', { name: '重置条件请求记录', exact: true })
    .click();
  await expect(page.getByRole('dialog')).toContainText(
    '保留配置、覆盖、同步错误和未知结果',
  );
  await expect(page.getByRole('dialog')).toContainText('条件记录损坏');
  await expect
    .poll(() =>
      page.evaluate(
        () => (window as unknown as { cacheResets: unknown[] }).cacheResets,
      ),
    )
    .toEqual([]);
  await page
    .getByRole('button', { name: '清理原订阅条件记录', exact: true })
    .click();
  await expect(page.getByRole('dialog').getByRole('status')).toContainText(
    '该订阅条件记录已清理，配置材料保留。',
  );
  await expect(
    page.getByRole('button', { name: '清理原订阅条件记录', exact: true }),
  ).toBeDisabled();
  const calls = await page.evaluate(() => {
    const win = window as unknown as {
      cacheResets: unknown[];
      manualSyncIds: string[];
    };
    return { cacheResets: win.cacheResets, manualSyncIds: win.manualSyncIds };
  });
  expect(calls.cacheResets).toEqual([{ id: 'cache', version: 1 }]);
  expect(calls.manualSyncIds).toEqual([]);
});

async function watchLocalRecovery(page: import('@playwright/test').Page) {
  await page.addInitScript(() => {
    const win = window as unknown as {
      localRecoveries: unknown[];
      recoveryMutations: unknown[];
    };
    win.localRecoveries = [];
    win.recoveryMutations = [];
    window.addEventListener('fixture-subscription-local-recovery', (event) =>
      win.localRecoveries.push((event as CustomEvent).detail),
    );
    for (const name of [
      'fixture-subscription-manual-sync',
      'fixture-subscription-cache-reset',
    ]) {
      window.addEventListener(name, (event) =>
        win.recoveryMutations.push((event as CustomEvent).detail),
      );
    }
  });
}

test('confirmed original receipt exposes explicit local save and refreshes the source only after confirmation', async ({
  page,
}) => {
  await watchLocalRecovery(page);
  await page.goto(
    '/?panel=subscriptions&mode=quota-facts&inspection=confirmed',
  );
  await page.getByRole('button', { name: '核对未知结果', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(
    dialog.getByRole('button', { name: '完成本地保存', exact: true }),
  ).toHaveCount(0);
  await page.getByRole('button', { name: '读取核对状态', exact: true }).click();
  await expect(dialog).toContainText('已有权威确认回执');
  await expect(dialog).toContainText('fixture-record');
  await expect(dialog).toContainText('不会重新下载、重新应用或扩大目标范围');
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { localRecoveries: unknown[] }).localRecoveries,
    ),
  ).toEqual([]);
  await dialog
    .getByRole('button', { name: '完成本地保存', exact: true })
    .click();
  await expect(dialog).toContainText('本地已保存');
  await expect(
    dialog.getByRole('button', { name: '完成本地保存', exact: true }),
  ).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as unknown as { localRecoveries: unknown[] }).localRecoveries,
      ),
    )
    .toEqual([
      {
        id: 'uncertain',
        expectedUpdatedAtUnixMs: '1',
        recoveryId: 'fixture-record',
      },
    ]);
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { recoveryMutations: unknown[] })
          .recoveryMutations,
    ),
  ).toEqual([]);
  await dialog.getByRole('button', { name: '返回', exact: true }).click();
  await expect(
    page.getByRole('button', { name: '核对未知结果', exact: true }),
  ).toHaveCount(0);
});

test('pending fixed local recovery prevents repeated submit and closing until original confirmation returns', async ({
  page,
}) => {
  await watchLocalRecovery(page);
  await page.goto(
    '/?panel=subscriptions&mode=quota-facts&inspection=recovery-late',
  );
  await page.getByRole('button', { name: '核对未知结果', exact: true }).click();
  await page.getByRole('button', { name: '读取核对状态', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog
    .getByRole('button', { name: '完成本地保存', exact: true })
    .click();
  await expect(dialog).toContainText('正在完成原候选的本地保存');
  await expect(
    dialog.getByRole('button', { name: '完成本地保存', exact: true }),
  ).toBeDisabled();
  await expect(
    dialog.getByRole('button', { name: '重新核对', exact: true }),
  ).toBeDisabled();
  await expect(
    dialog.getByRole('button', { name: '返回', exact: true }),
  ).toBeDisabled();
  await expect(
    dialog.getByRole('button', { name: '关闭', exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(dialog).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as unknown as { localRecoveries: unknown[] }).localRecoveries
            .length,
      ),
    )
    .toBe(1);
  await page.evaluate(() =>
    window.dispatchEvent(new Event('fixture-release-recovery')),
  );
  await expect(dialog).toContainText('本地已保存');
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { localRecoveries: unknown[] }).localRecoveries,
    ),
  ).toEqual([
    {
      id: 'uncertain',
      expectedUpdatedAtUnixMs: '1',
      recoveryId: 'fixture-record',
    },
  ]);
});

test('failed local publication preserves evidence and does not automatically replay or clear uncertainty', async ({
  page,
}) => {
  await watchLocalRecovery(page);
  await page.goto(
    '/?panel=subscriptions&mode=quota-facts&inspection=recovery-fail',
  );
  await page.getByRole('button', { name: '核对未知结果', exact: true }).click();
  await page.getByRole('button', { name: '读取核对状态', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog
    .getByRole('button', { name: '完成本地保存', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toContainText(
    '本地保存未确认，原核对记录保留',
  );
  await expect(dialog).toContainText('已有权威确认回执');
  await expect(dialog).toContainText('fixture-record');
  await expect(
    dialog.getByRole('button', { name: '完成本地保存', exact: true }),
  ).toBeDisabled();
  await expect(
    dialog.getByRole('button', { name: '重新核对', exact: true }),
  ).toBeEnabled();
  await expect(dialog).not.toContainText('private-canary');
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { localRecoveries: unknown[] }).localRecoveries
          .length,
    ),
  ).toBe(1);
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { recoveryMutations: unknown[] })
          .recoveryMutations,
    ),
  ).toEqual([]);
  await dialog.getByRole('button', { name: '返回', exact: true }).click();
  await expect(
    page
      .locator('.list-row')
      .filter({ has: page.getByText('原运行提交待核对', { exact: true }) }),
  ).toContainText('结果待核对');
});

test('prepared receipt-less or stale records remain inspection-only despite a recorded candidate', async ({
  page,
}) => {
  await watchLocalRecovery(page);
  for (const inspection of ['prepared', 'stale']) {
    await page.goto(
      `/?panel=subscriptions&mode=quota-facts&inspection=${inspection}`,
    );
    await page
      .getByRole('button', { name: '核对未知结果', exact: true })
      .click();
    await page
      .getByRole('button', { name: '读取核对状态', exact: true })
      .click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toContainText('已有持久记录');
    await expect(dialog).toContainText('fixture-record');
    await expect(
      dialog.getByRole('button', { name: '完成本地保存', exact: true }),
    ).toHaveCount(0);
    await expect(dialog).toContainText('结果仍未确认');
    expect(
      await page.evaluate(
        () =>
          (window as unknown as { localRecoveries: unknown[] }).localRecoveries,
      ),
    ).toEqual([]);
  }
});
