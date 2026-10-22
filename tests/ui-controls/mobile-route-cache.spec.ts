import { test, expect } from '@playwright/test';
test.use({viewport:{width:320,height:740}});
const facts={target:'example.test',port:443,protocol:'tcp',inbound_tag:'web',effective_mode:'rule',route_action:{route:'manual-group'},matched_rule:{index:'18446744073709551615',condition:'domain_suffix example.test'}};
async function complete(page: import('@playwright/test').Page,kind:string,result:unknown){await page.evaluate(({kind,result})=>(window as unknown as {a19Fixture:{complete(k:string,r:unknown):void}}).a19Fixture.complete(kind,result),{kind,result});}
async function requests(page: import('@playwright/test').Page){return page.evaluate(()=>(window as unknown as {a19Fixture:{requests:unknown[]}}).a19Fixture.requests);}
async function startRoute(page:import('@playwright/test').Page){await page.goto('/a19-diagnostics.html?tool=route');await page.getByLabel('评估目标',{exact:true}).fill('example.test');await page.getByLabel('评估端口',{exact:true}).fill('443');await page.getByLabel('评估入站标签',{exact:true}).fill('web');await page.getByRole('button',{name:'评估',exact:true}).click();}

test('actual route panel displays matched rule, action and missing chain without invented hop facts',async({page})=>{
 await startRoute(page);await complete(page,'route_trace',{...facts,hops:[{ttl:1,rtt:3}],private_extra:'must-not-copy'});
 const result=page.getByLabel('当前路由评估结果');await expect(result).toContainText('路由目标：manual-group');await expect(result).toContainText('18446744073709551615');await expect(result).toContainText('domain_suffix example.test');await expect(result).toContainText('当前接口未提供');
 await expect(page.getByRole('columnheader',{name:'TTL'})).toHaveCount(0);await expect(page.getByRole('columnheader',{name:'RTT'})).toHaveCount(0);
 await page.getByRole('button',{name:'复制评估结果'}).click();await expect(page.getByText('已复制评估结果')).toBeVisible();
 const copies=await page.evaluate(()=>(window as unknown as {a19Fixture:{copies:string[]}}).a19Fixture.copies);expect(copies).toHaveLength(1);expect(copies[0]).not.toContain('must-not-copy');expect(copies[0]).not.toContain('hops');
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 for(const button of await page.locator('.diag-tool button').all())expect((await button.boundingBox())?.height).toBeGreaterThanOrEqual(44);
});
test('route final direct and rejection are explicit and unsupported result has no raw fallback',async({page})=>{
 await startRoute(page);await complete(page,'route_trace',{...facts,route_action:'reject',matched_rule:null});await expect(page.getByLabel('当前路由评估结果')).toContainText('拒绝');await expect(page.getByLabel('当前路由评估结果')).toContainText('未命中具体规则');
 await page.getByRole('button',{name:'评估',exact:true}).click();await complete(page,'route_trace',{...facts,route_action:'direct',matched_rule:null});await expect(page.getByLabel('当前路由评估结果')).toContainText('直连');
 await page.getByRole('button',{name:'评估',exact:true}).click();await complete(page,'route_trace',{hops:[],secret:'do-not-display'});await expect(page.getByText(/内核路由评估结果不完整/)).toBeVisible();await expect(page.getByLabel('当前路由评估结果')).toHaveCount(0);await expect(page.getByText('do-not-display',{exact:true})).toHaveCount(0);
});
test('route validates input and keeps backend permission denial explicit without retrying',async({page})=>{
 await page.goto('/a19-diagnostics.html?tool=route');await page.getByLabel('评估目标',{exact:true}).fill('example.test');await page.getByLabel('评估端口',{exact:true}).fill('0');await page.getByRole('button',{name:'评估',exact:true}).click();await expect(page.getByText(/端口必须为/)).toBeVisible();expect(await requests(page)).toHaveLength(0);
 await page.getByLabel('评估端口',{exact:true}).fill('443');await page.evaluate(()=>(window as unknown as {a19Fixture:{deny(v:boolean):void}}).a19Fixture.deny(true));await page.getByRole('button',{name:'评估',exact:true}).click();await expect(page.getByText(/没有 Admin/)).toBeVisible();expect(await requests(page)).toHaveLength(1);
});
test('standalone cache reads list without lookup or FakeIP and preserves TTL on narrow screens',async({page})=>{
 await page.goto('/a19-diagnostics.html');const cache=page.getByRole('region',{name:'DNS 缓存',exact:true});await cache.getByLabel('缓存列表上限').fill('20');await cache.getByRole('button',{name:'读取缓存'}).click();expect(await requests(page)).toMatchObject([{kind:'dns_cache',params:{limit:20}}]);
 await complete(page,'dns_cache',{enabled:true,entries:[{domain:'very-long-public-domain.example.test',addresses:['2001:db8::53','192.0.2.53'],ttl_seconds:'18446744073709551615'}],count:1});await expect(cache).toContainText('18446744073709551615');await expect(cache).toContainText('不是缓存总量');expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 for(const button of await cache.locator('button').all())expect((await button.boundingBox())?.height).toBeGreaterThanOrEqual(44);
});
test('cache miss, disabled and malformed facts are distinct and explicit retry preserves domain',async({page})=>{
 await page.goto('/a19-diagnostics.html');const cache=page.getByRole('region',{name:'DNS 缓存',exact:true});await cache.getByLabel('缓存域名').fill('missing.test');await cache.getByRole('button',{name:'读取缓存'}).click();await complete(page,'dns_cache',{enabled:true,domain:'missing.test',hit:false,addresses:[],ttl_seconds:null});await expect(cache).toContainText('未命中缓存；没有发起新解析');
 await cache.getByRole('button',{name:'读取缓存'}).click();await complete(page,'dns_cache',{enabled:false,entries:[],count:0});await expect(cache).toContainText('内核缓存未启用');
 await cache.getByRole('button',{name:'读取缓存'}).click();await complete(page,'dns_cache',{enabled:true});await expect(cache).toContainText('没有推断为空缓存或零计数');await expect(cache.getByLabel('当前 DNS 缓存结果')).toHaveCount(0);await expect(cache.getByLabel('缓存域名')).toHaveValue('missing.test');
});
test('scope replacement clears route/cache facts and late completion cannot repopulate them',async({page})=>{
 for(const tool of ['route','dns']){if(tool==='route')await startRoute(page);else{await page.goto('/a19-diagnostics.html');await page.getByRole('button',{name:'读取缓存'}).click();}
 const kind=tool==='route'?'route_trace':'dns_cache',result=tool==='route'?facts:{enabled:true,entries:[],count:0};await complete(page,kind,result);const label=tool==='route'?'当前路由评估结果':'当前 DNS 缓存结果';await expect(page.getByLabel(label)).toBeVisible();await page.evaluate(()=>(window as unknown as {a19Fixture:{change():void}}).a19Fixture.change());await expect(page.getByLabel(label)).toHaveCount(0);await complete(page,kind,result);await expect(page.getByLabel(label)).toHaveCount(0);expect(await requests(page)).toHaveLength(1);}
});
