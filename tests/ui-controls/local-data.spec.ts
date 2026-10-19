import {test, expect} from '@playwright/test';
test.use({viewport:{width:320,height:740}});
test('local data distinguishes real zero, lossless bytes, unavailable capacity and stale metadata',async({page})=>{
  await page.goto('/?panel=local-data');
  await expect(page.getByRole('region',{name:'日志页记录容量'})).toContainText('0 字节');
  await expect(page.getByRole('region',{name:'本地完成连接历史容量'})).toContainText('18446744073709551615 字节');
  await expect(page.getByRole('region',{name:'本地完成连接历史容量'})).toContainText('30 天');
  await expect(page.getByRole('region',{name:'捕获帧文件容量'})).toContainText('—（不可读取）');
  await page.evaluate(()=>(window as any).localDataFixture.fail=true);
  await page.getByRole('button',{name:'重新读取容量与保留预算'}).click();
  await expect(page.getByRole('alert')).toContainText('上次采样仅供历史查看');
  await expect(page.locator('body')).toContainText('已过期');
  await expect(page.locator('body')).not.toContainText('SECRET');
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
});
test('cleanup uses only the explicitly confirmed local history scope and refreshes actual metadata',async({page})=>{
  await page.goto('/?panel=local-data');
  await page.getByRole('button',{name:'清理本地完成连接历史',exact:true}).click();
  await page.getByRole('button',{name:'取消',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).localDataFixture.calls.filter((call:string)=>call.startsWith('gui_debug_clear')))).toEqual([]);
  await page.getByRole('button',{name:'清理本地完成连接历史',exact:true}).click();
  await page.getByRole('button',{name:'确认清理',exact:true}).click();
  await expect(page.getByRole('status')).toContainText('本地完成连接历史清理已确认');
  expect(await page.evaluate(()=>(window as any).localDataFixture.calls.filter((call:string)=>call.startsWith('gui_debug_clear')))).toEqual(['gui_debug_clear:connection-history']);
});
