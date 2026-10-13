import {test,expect,type Page} from '@playwright/test';
async function openAbout(page:Page,fail=false){
 await page.setViewportSize({width:320,height:740});
 await page.addInitScript(({fail})=>{
  const win=window as unknown as {__aboutCalls:{command:string;args?:Record<string,unknown>}[];__TAURI_INTERNALS__:{invoke:(command:string,args?:Record<string,unknown>)=>Promise<unknown>}};
  win.__aboutCalls=[];
  win.__TAURI_INTERNALS__={invoke:async(command,args)=>{
   win.__aboutCalls.push({command,args});
   if(command==='platform_open_url' && fail)throw new Error('platform browser denied');
   const values:Record<string,unknown>={'plugin:app|name':'ZNet Sink native fixture','plugin:app|version':'0.0.3-native-fixture','plugin:app|identifier':'org.zerodenet.znetsink'};
   return values[command];
  }};
 },{fail});
 await page.goto('/mobile-about.html');await expect(page.getByText('0.0.3-native-fixture',{exact:true})).toBeVisible();
}
test('320px About renders actual app metadata and complete repository license without implicit external actions',async({page})=>{
 await openAbout(page);
 await expect(page.getByText('ZeroDenet',{exact:true})).toBeVisible();
 await expect(page.getByText('平台尚未提供真实运行内核的版本回执。',{exact:false})).toBeVisible();
 const calls=await page.evaluate(()=> (window as unknown as {__aboutCalls:{command:string}[]}).__aboutCalls.map(c=>c.command));
 expect(calls).toEqual(['plugin:app|name','plugin:app|version','plugin:app|identifier']);
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
