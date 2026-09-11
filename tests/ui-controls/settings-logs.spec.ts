import { test, expect } from '@playwright/test';

test('settings rows and switches stay inside the actual section container', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/?panel=settings');
  await expect(page.getByText('界面与窗口', { exact: true })).toBeVisible();
  const control = page.getByRole('switch', { name: '流量悬浮球' });
  await expect(control).toBeVisible();
  const size = await control.boundingBox();
  expect(size?.width).toBeGreaterThan(25);
  expect(size?.width).toBeLessThan(50);
  await page.screenshot({ path: test.info().outputPath('settings-general.png') });
  for (const width of [900, 700, 520]) {
    await page.setViewportSize({ width, height: 650 });
    await page.getByRole('navigation', { name: '设置' }).getByRole('button', { name: '网络', exact: true }).click();
    await expect(page.getByText('代理端口', { exact: true })).toBeVisible();
    const overflow = await page.locator('.section-content').evaluate(el => el.scrollWidth - el.clientWidth);
    expect(overflow).toBeLessThanOrEqual(1);
  }
  for (const width of [932, 900, 700, 520]) {
    await page.setViewportSize({width,height:650});
    for (const dark of [false, true]) {
      await page.evaluate(dark => document.documentElement.classList.toggle('dark',dark),dark);
      for (const section of ['应用','网络','日志','域名解析','流量接管']) {
        await page.getByRole('navigation',{name:'设置'}).getByRole('button',{name:section,exact:true}).click();
        await expect(page.getByText('加载配置中...', {exact:true})).toHaveCount(0);
        await expect(page.locator('.section-content')).not.toBeEmpty();
        await expect(page.locator('.section-content [role=alert]')).toHaveCount(0);
        if (section === '域名解析') {
          await expect(page.getByRole('radio',{name:'Fake-IP',exact:true})).toBeVisible();
          await page.getByRole('radio',{name:'Fake-IP',exact:true}).click();
          await expect(page.getByRole('radio',{name:'Fake-IP',exact:true})).toHaveAttribute('aria-checked','true');
        }
        await expect.poll(()=>page.locator('.section-content').evaluate(el=>el.scrollWidth-el.clientWidth),{message:`${section} ${width}px overflow`}).toBeLessThanOrEqual(1);
        for (const item of await page.locator('.section-content [role="switch"]').all()) {
          const size = await item.boundingBox();
          expect(size?.width,`${section} switch width`).toBeLessThan(50);
        }
      }
    }
  }
  await page.screenshot({path:test.info().outputPath('settings-tun.png')});
  expect(errors).toEqual([]);
});

test('advanced configuration mounts without a bindable ref crash', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/?panel=settings&section=config');
  await expect(page.getByText('内核配置编辑', { exact: true })).toBeVisible();
  await expect(page.getByText('页面显示异常', { exact: true })).toHaveCount(0);
  await expect(page.locator('.editor-container textarea')).toBeVisible();
  expect(errors).toEqual([]);
});

test('logs remain interactive with large structured records and repeated refreshes', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    const samples: number[] = [];
    (window as any).longTasks = samples;
    if (PerformanceObserver.supportedEntryTypes.includes('longtask')) new PerformanceObserver(list => samples.push(...list.getEntries().map(e => e.duration))).observe({ type: 'longtask', buffered: true });
  });
  await page.goto('/?panel=logs');
  await expect(page.locator('.log-row').first()).toBeVisible();
  await expect.poll(()=>page.locator('.log-row').count()).toBeLessThan(50);
  await expect(page.getByLabel('已加载日志摘要')).toContainText('400+');
  await page.locator('.log-summary').first().click();
  await expect(page.locator('.log-details')).toContainText('session_id');
  await page.getByRole('button', { name: '立即刷新', exact: true }).click();
  await expect(page.locator('.log-details')).toBeVisible();
  await page.locator('.log-summary').first().click();
  // Scrolling reaches records outside the initial DOM window.
  await page.locator('.log-body').evaluate(el => el.scrollTop = 5000);
  await expect.poll(()=>page.locator('.log-row').first().getAttribute('data-log-id')).not.toBe('5000');
  const anchor=await page.locator('.log-row').first().getAttribute('data-log-id');
  await page.evaluate(()=>window.dispatchEvent(new Event('fixture-append-log')));
  await page.getByRole('button',{name:'立即刷新',exact:true}).click();
  await expect(page.locator(`[data-log-id="${anchor}"]`)).toHaveCount(1);
  await page.locator('.log-body').evaluate(el => el.scrollTop = el.scrollHeight);
  await page.getByRole('button',{name:'加载更早日志',exact:true}).click();
  await expect(page.getByLabel('已加载日志摘要')).toContainText('801');
  await expect.poll(()=>page.locator('.log-row').count()).toBeLessThan(50);
  await page.getByPlaceholder('搜索日志（Ctrl+F）').fill('session_id');
  await expect(page.locator('.log-row').first()).toBeVisible();
  await page.getByPlaceholder('搜索日志（Ctrl+F）').fill('日志 4600 ');
  await expect(page.locator('.log-row')).toHaveCount(1);
  await expect(page.locator('.log-message')).toContainText('日志 4600 ');
  await page.getByPlaceholder('搜索日志（Ctrl+F）').fill('');
  await page.getByTitle('暂停实时刷新',{exact:true}).click();
  await expect(page.locator('.live-status')).toContainText('已暂停');
  await page.getByRole('button', { name: '切到设置', exact: true }).click();
  await expect(page.getByText('界面与窗口', { exact: true })).toBeVisible();
  console.log('log mount/refresh/unmount long tasks', await page.evaluate(() => (window as any).longTasks));
  expect(errors).toEqual([]);
});

test('overview displays the bundled flag and readable network region',async ({page})=>{
  await page.goto('/?panel=overview');
  await expect(page.getByText('美国 · California · Los Angeles',{exact:true})).toBeVisible();
  const flag=page.getByRole('img',{name:'国旗 US'});
  await expect(flag).toBeVisible();
  expect(await flag.evaluate(el=>getComputedStyle(el).backgroundImage)).toMatch(/us\.svg|flag-icons-us/);
});
