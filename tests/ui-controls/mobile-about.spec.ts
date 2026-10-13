import {test,expect,type Page} from '@playwright/test';
const inventory={schemaVersion:1,target:{platform:'frontend',triple:'browser',profile:'production',features:[]},provenance:{kind:'synthetic-test-fixture'},entries:[],texts:[],gaps:[]};
async function openAbout(page:Page,fail=false){
 await page.route('**/license-inventory.frontend.json',route=>route.fulfill({json:inventory}));
 await page.setViewportSize({width:320,height:740});
 await page.addInitScript(({fail,inventory})=>{
  const win=window as unknown as {__aboutCalls:{command:string;args?:Record<string,unknown>}[];__TAURI_INTERNALS__:{invoke:(command:string,args?:Record<string,unknown>)=>Promise<unknown>}};
  win.__aboutCalls=[];
  win.__TAURI_INTERNALS__={invoke:async(command,args)=>{
   win.__aboutCalls.push({command,args});
   if(command==='app_license_inventory')return {schemaVersion:1,rust:{...inventory,target:{...inventory.target,platform:'desktop',triple:'fixture-host'}},native:{status:'available',inventory:{...inventory,target:{...inventory.target,platform:'android',triple:'fixture-android'}},reason:null}};
   if(command==='platform_open_url' && fail)throw new Error('platform browser denied');
   const values:Record<string,unknown>={'plugin:app|name':'ZNet Sink native fixture','plugin:app|version':'0.0.3-native-fixture','plugin:app|identifier':'org.zerodenet.znetsink'};
   return values[command];
  }};
 },{fail,inventory});
 await page.goto('/mobile-about.html');await expect(page.getByText('0.0.3-native-fixture',{exact:true})).toBeVisible();
}
test('320px About renders actual app metadata and complete repository license without implicit external actions',async({page})=>{
 await openAbout(page);
 await expect(page.getByText('ZeroDenet',{exact:true})).toBeVisible();
 await expect(page.getByText('平台尚未提供真实运行内核的版本回执。',{exact:false})).toBeVisible();
 const calls=await page.evaluate(()=> (window as unknown as {__aboutCalls:{command:string}[]}).__aboutCalls.map(c=>c.command));
 expect(calls.sort()).toEqual(['app_license_inventory','plugin:app|name','plugin:app|version','plugin:app|identifier'].sort());
 await page.getByText('Mozilla Public License 2.0 全文',{exact:true}).click();
 await expect(page.getByText('Exhibit B - "Incompatible With Secondary Licenses" Notice',{exact:false})).toBeVisible();
 const dimensions=await page.evaluate(()=>({content:document.documentElement.scrollWidth,viewport:innerWidth}));expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport);
 const buttons=await page.getByRole('button').all();for(const button of buttons){const box=await button.boundingBox();if(box)expect(box.height).toBeGreaterThanOrEqual(44);}
});
test('feedback click invokes platform browser with project issues URL and no automatic submission',async({page})=>{
 await openAbout(page);await page.getByRole('button',{name:'问题反馈',exact:true}).click();
 await expect(page.getByRole('status')).toHaveText('已交给系统浏览器。');
 const commands=await page.evaluate(()=> (window as unknown as {__aboutCalls:{command:string;args?:Record<string,unknown>}[]}).__aboutCalls);
 expect(commands.at(-1)).toEqual({command:'platform_open_url',args:{url:'https://github.com/zerodenet/znet-sink/issues'}});
 expect(commands.filter(c=>c.command.includes('diagnostic') || c.command.includes('upload'))).toHaveLength(0);
});
test('browser refusal exposes retained exact address and a separate real host clipboard action',async({page})=>{
 await openAbout(page,true);await page.getByRole('button',{name:'官网',exact:true}).click();
 await expect(page.getByLabel('未打开的项目地址')).toHaveText('https://zerodenet.org');
 await page.getByRole('button',{name:'复制地址',exact:true}).click();await expect(page.getByRole('status')).toHaveText('地址已复制。');
 const last=await page.evaluate(()=> (window as unknown as {__aboutCalls:{command:string;args?:Record<string,unknown>}[]}).__aboutCalls.at(-1));
 expect(last).toEqual({command:'platform_clipboard_write',args:{text:'https://zerodenet.org'}});
});

test('license search uses built records and retains explicit unresolved material gaps',async({page})=>{
 await openAbout(page);
 await page.unroute('**/license-inventory.frontend.json');
 await page.route('**/license-inventory.frontend.json',route=>route.fulfill({json:{...inventory,entries:[{id:'fixture:dependency',name:'Fixture dependency',version:'1.2.3',ecosystem:'npm',declaredLicense:'MIT',source:'actual-test-fixture',evidence:[{kind:'bundled',detail:'Synthetic module/chunk proof'}],licenseTextIds:[],noticeTextIds:[],textStatus:'missing',missing:['license-text-not-provided-by-installed-material']}],gaps:[{reason:'license-text-not-provided-by-installed-material'}]}}));
 await page.getByRole('button',{name:'重新读取许可目录'}).click();
 await page.getByText('Fixture dependency · MIT',{exact:true}).click();
 await expect(page.getByText('资料缺口：license-text-not-provided-by-installed-material')).toBeVisible();
 await expect(page.getByText('前端产物包含：Synthetic module/chunk proof')).toBeVisible();
 await page.getByLabel('搜索依赖许可').fill('unrelated');
 await expect(page.getByText('显示 0 个依赖')).toBeVisible();
});
