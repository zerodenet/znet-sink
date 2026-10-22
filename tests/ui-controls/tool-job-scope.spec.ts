import {test,expect} from '@playwright/test';
test('restored tool history stays historical and does not start a fresh cache read',async({page})=>{
 await page.goto('/tool-job-scope.html');
 const dns=page.getByRole('region',{name:'DNS 页面'});
 await expect(dns.getByText('任务历史（2）')).toBeVisible();
 await expect(page.getByLabel('任务请求')).toHaveText('[]');
 await expect(dns.getByText(/已删除 4 个映射/)).toHaveCount(0);
 await page.getByRole('button',{name:'旧清理事件'}).click();
 await expect(page.getByLabel('任务请求')).toHaveText('[]');
 await dns.getByText('任务历史（2）').click();
 await dns.getByText('#1 · DNS 查询 · 完成',{exact:true}).click();
 await expect(dns.getByText('192.0.2.99',{exact:false})).toBeVisible();
 await expect(dns.getByText('历史只用于追溯，不代表当前网络状态，不会自动重试或触发后续请求。')).toBeVisible();
});
test('current DNS result is cleared on profile or instance change, including late completion',async({page})=>{
 await page.goto('/tool-job-scope.html');
 const dns=page.getByRole('region',{name:'DNS 页面'});
 await dns.getByPlaceholder('example.com',{exact:true}).fill('current.example');
 await dns.locator('.diag-tool').first().getByRole('button',{name:'查询',exact:true}).click();
 await page.getByRole('button',{name:'完成当前 DNS'}).click();
 await expect(dns.getByText('192.0.2.4',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'切换配置',exact:true}).click();
 await expect(dns.getByText('192.0.2.4',{exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'完成当前 DNS'}).click();
 await expect(dns.getByText('192.0.2.4',{exact:true})).toHaveCount(0);
 await dns.locator('.diag-tool').first().getByRole('button',{name:'查询',exact:true}).click();
 await page.getByRole('button',{name:'完成当前 DNS'}).click();
 await expect(dns.getByText('192.0.2.4',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'重启内核',exact:true}).click();
 await expect(dns.getByText('192.0.2.4',{exact:true})).toHaveCount(0);
 await expect(page.getByLabel('任务请求')).toContainText('18446744073709551615');
});
test('a live clear issues one scope-bound cache read and remains usable on a 320px screen',async({page})=>{
 await page.setViewportSize({width:320,height:740});await page.goto('/tool-job-scope.html');
 const dns=page.getByRole('region',{name:'DNS 页面'});
 await dns.getByPlaceholder('open.bigmodel.cn',{exact:true}).fill('current.example');
 await dns.getByRole('button',{name:'删除当前映射',exact:true}).click();
 await page.getByRole('button',{name:'完成当前清理'}).click();
 await expect(dns.getByText(/已删除 1 个映射/)).toBeVisible();
 await expect(page.getByLabel('任务请求')).toContainText('dns_cache');
 await page.getByRole('button',{name:'完成当前清理'}).click();
 const text=await page.getByLabel('任务请求').textContent();
 expect(JSON.parse(text!).filter((r:{kind:string})=>r.kind==='dns_cache')).toHaveLength(1);
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
});


test('an imprecise task ID is explained and never offered as a current result or stop target',async({page})=>{
 await page.goto('/tool-job-scope.html');
 const dns=page.getByRole('region',{name:'DNS 页面'});
 await page.getByRole('button',{name:'超范围任务事件'}).click();
 await expect(dns.getByText(/部分任务身份无法无损表示/)).toBeVisible();
 await expect(dns.getByText(/#9007199254740992/)).toHaveCount(0);
 await expect(page.getByLabel('任务请求')).toHaveText('[]');
 await expect(dns.getByText('任务历史（2）')).toBeVisible();
});
