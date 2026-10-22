import { test, expect, type Page } from '@playwright/test';

async function fixture(page: Page, checked = true) {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.addInitScript(({ checked }) => {
    const calls: {command:string; args:any}[] = [];
    let generation = '1';
    const component = {
      plugin_id:'org.example.mobile',component_id:'main',name:'移动插件',description:'前台组件',version:'1.0.0',publisher:'example',repository:'https://github.com/example/mobile',license:'MIT',surfaces:[],
      review:{key:'org.example.mobile/main',identity:'original',registration:1,revision:0,host_generation:generation},
      permissions:[],enabled:false,permission_review_required:false,running:false,blocked:null,
      configuration:{configured:true,schema:{title:'来源设置',fields:[{id:'label',label:'来源名称',kind:'text',required:true}]},values:{label:'已确认'}},
    };
    const snapshot = () => ({checked,components:checked?[structuredClone(component)]:[],notices:[],platform:{state:'ready',target:{os:'android',arch:'aarch64',device:'phone'},generation,executionAllowed:true,externalInstallationAllowed:true,isolation:'bounded-in-process-interpreter',reason:null,taskScheduling:{mode:'foreground-only',state:'available',reason:'仅在应用前台核对到期任务；后台执行尚未适配，不保证执行时间。结果未确认的任务不会自动重试'}}});
    const review = {host_generation:generation,plugin_id:'org.example.local',current_version:null,candidate_version:'1.0.0',candidate_digest:'signed-package-digest',publisher:'local-publisher',publisher_fingerprint:'original-publisher-fingerprint',first_install:true,local_trust:true,requested_surfaces:[],requires_approval:true,added_permissions:[],removed_permissions:[]};
    (window as any).mobilePlugin = { calls, changeHost() {
      generation='2'; component.review={...component.review,identity:'replacement',registration:2,revision:1,host_generation:generation};
      component.configuration.values.label='新的已确认值';
      window.dispatchEvent(new CustomEvent('plugins:changed'));
    }};
    (window as any).__TAURI_INTERNALS__ = {
      invoke:async (command:string,args:any={}) => {
        calls.push({command,args:JSON.parse(JSON.stringify(args))});
        if(command==='plugins_supported')return true;
        if(command==='plugins_snapshot'||command==='plugins_refresh'||command==='plugins_host_recheck')return snapshot();
        if(command==='plugins_catalog'||command==='plugins_releases'||command==='plugins_uninstall_recent')return [];
        if(command==='plugins_configure') {
          if(args.review.host_generation!==generation)throw {code:'plugin_review_stale',message:'原生宿主已变化，请重新审阅；草稿保留'};
          component.configuration.values=args.values;component.review.revision++;return snapshot();
        }
        if(command==='plugins_native_package_pick') {
          if((window as any).mobilePlugin.pickError)throw (window as any).mobilePlugin.pickError;
          return (window as any).mobilePlugin.pickCancelled?null:{plan_id:'original-plan',review};
        }
        if(command==='plugins_native_package_inspect')return {pluginId:review.plugin_id,state:(window as any).mobilePlugin.inspectionState??'matching',installedVersion:'1.0.0',packageDigest:review.candidate_digest,observedAtUnixMs:'1792431000000'};
        if(command==='plugins_native_package_cancel')return;
        if(command==='plugins_native_package_confirm') {
          if(args.planId!=='original-plan'||args.approvalDigest!=='signed-package-digest')throw Error('wrong original plan');
          if((window as any).mobilePlugin.confirmError)throw (window as any).mobilePlugin.confirmError;
          return snapshot();
        }
        if(command==='plugins_preview_install'||command==='plugins_install'||command==='plugin:dialog|open')throw Error('desktop path must not be used');
        if(command==='app_config_get')return {tun:{enabled:false}};
        return null;
      }, transformCallback:()=>1, unregisterCallback:()=>{},
    };
  }, { checked });
  await page.goto('/?panel=plugins');
}

test('native change keeps the original configuration review and draft until explicit reload', async ({ page }) => {
  await fixture(page);
  await page.getByRole('button',{name:'打开 移动插件 详情',exact:true}).click();
  const input=page.getByRole('textbox',{name:'来源名称',exact:true});
  await input.fill('仍在编辑的草稿');
  await page.evaluate(()=>(window as any).mobilePlugin.changeHost());
  await expect(input).toHaveValue('仍在编辑的草稿');
  await page.getByRole('button',{name:'保存配置',exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('草稿保留');
  expect(await page.evaluate(()=>(window as any).mobilePlugin.calls.find((call:any)=>call.command==='plugins_configure').args.review.host_generation)).toBe('1');
  await expect(input).toHaveValue('仍在编辑的草稿');
  await page.getByRole('button',{name:'放弃草稿并重新载入',exact:true}).click();
  await expect(input).toHaveValue('新的已确认值');
  await page.getByRole('button',{name:'保存配置',exact:true}).click();
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('native package selection reviews publisher and confirms only opaque original plan', async ({ page }) => {
  await fixture(page);
  await page.getByRole('button',{name:'安装插件',exact:true}).click();
  await page.getByRole('button',{name:/本地/}).click();
  await expect(page.getByText('original-publisher-fingerprint',{exact:true})).toBeVisible();
  expect(await page.evaluate(()=>(window as any).mobilePlugin.calls.filter((call:any)=>call.command==='plugins_native_package_confirm').length)).toBe(0);
  await page.getByRole('button',{name:'信任并安装',exact:true}).click();
  const calls=await page.evaluate(()=>(window as any).mobilePlugin.calls);
  expect(calls.find((call:any)=>call.command==='plugins_native_package_confirm').args).toEqual({planId:'original-plan',approvalDigest:'signed-package-digest'});
  expect(calls.some((call:any)=>['plugins_preview_install','plugins_install','plugin:dialog|open'].includes(call.command))).toBe(false);
});

test('an unverified native catalog is not authoritative empty installation', async ({ page }) => {
  await fixture(page,false);
  await expect(page.getByText('安装状态尚未核对',{exact:true})).toBeVisible();
  await expect(page.getByText('还没有已安装的插件',{exact:true})).toHaveCount(0);
});

test('native selection uncertainty remains visible without a partial plan or installation', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { (window as any).mobilePlugin.pickError={code:'plugin_native_selection_unconfirmed',message:'系统文件读取结果未确认；不会安装或自动重试'}; });
  await page.getByRole('button',{name:'安装插件',exact:true}).click();
  await page.getByRole('button',{name:/本地/}).click();
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('读取结果未确认');
  await expect(page.getByRole('button',{name:'信任并安装',exact:true})).toHaveCount(0);
  expect(await page.evaluate(()=>(window as any).mobilePlugin.calls.filter((call:any)=>call.command==='plugins_native_package_confirm').length)).toBe(0);
});

test('native selection cancellation preserves the original page without an error or plan', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { (window as any).mobilePlugin.pickCancelled=true; });
  await page.getByRole('button',{name:'安装插件',exact:true}).click();
  await page.getByRole('button',{name:/本地/}).click();
  await expect(page.getByRole('button',{name:/本地/})).toBeEnabled();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('button',{name:'信任并安装',exact:true})).toHaveCount(0);
});

test('a consumed native confirmation is never replayed and inspection retains the original target', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { (window as any).mobilePlugin.confirmError={code:'plugin_install_result_unconfirmed',message:'安装结果未确认，请重新读取'}; });
  await page.getByRole('button',{name:'安装插件',exact:true}).click();
  await page.getByRole('button',{name:/本地/}).click();
  await page.getByRole('button',{name:'信任并安装',exact:true}).click();
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('安装结果未确认');
  await expect(page.getByRole('button',{name:'信任并安装',exact:true})).toBeDisabled();
  await page.evaluate(()=>(window as any).mobilePlugin.changeHost());
  await page.getByRole('button',{name:'重新读取安装状态',exact:true}).click();
  await expect(page.getByText(/已核对安装材料：版本 1.0.0/)).toBeVisible();
  const calls=await page.evaluate(()=>(window as any).mobilePlugin.calls);
  expect(calls.filter((call:any)=>call.command==='plugins_native_package_confirm')).toHaveLength(1);
  expect(calls.find((call:any)=>call.command==='plugins_native_package_inspect').args.target).toEqual({pluginId:'org.example.local',candidateDigest:'signed-package-digest',candidateVersion:'1.0.0',publisherFingerprint:'original-publisher-fingerprint'});
  expect(calls.some((call:any)=>call.command==='plugins_refresh')).toBe(false);
});

test('a different installed package is reported without claiming or replaying the intended installation', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => { Object.assign((window as any).mobilePlugin,{confirmError:{message:'结果未确认'},inspectionState:'different'}); });
  await page.getByRole('button',{name:'安装插件',exact:true}).click();
  await page.getByRole('button',{name:/本地/}).click();
  await page.getByRole('button',{name:'信任并安装',exact:true}).click();
  await page.getByRole('button',{name:'重新读取安装状态',exact:true}).click();
  await expect(page.getByText(/与原审阅材料不同；不会替换或重放安装/)).toBeVisible();
  await expect(page.getByText(/已核对安装材料/)).toHaveCount(0);
  expect(await page.evaluate(()=>(window as any).mobilePlugin.calls.filter((call:any)=>call.command==='plugins_native_package_confirm').length)).toBe(1);
  await page.getByRole('button',{name:'返回',exact:true}).click();
  expect(await page.evaluate(()=>(window as any).mobilePlugin.calls.filter((call:any)=>call.command==='plugins_native_package_cancel').map((call:any)=>call.args.planId))).toEqual(['original-plan']);
});

for (const theme of ['light','dark']) test(`native installation recovery stays scrollable at 320px with enlarged ${theme} text`, async ({ page }) => {
  await fixture(page);
  await page.evaluate(theme => {
    Object.assign((window as any).mobilePlugin,{confirmError:{message:'安装结果未确认，请重新读取'}});
    document.documentElement.classList.toggle('dark',theme==='dark');
    document.documentElement.style.fontSize='20px';
  },theme);
  await page.getByRole('button',{name:'安装插件',exact:true}).click();
  await page.getByRole('button',{name:/本地/}).click();
  await page.getByRole('button',{name:'信任并安装',exact:true}).click();
  const inspect=page.getByRole('button',{name:'重新读取安装状态',exact:true});
  await inspect.scrollIntoViewIfNeeded();
  await expect(inspect).toBeVisible();
  expect((await inspect.boundingBox())!.height).toBeGreaterThanOrEqual(44);
  const dialog=page.getByRole('dialog');
  expect(await dialog.evaluate(e=>e.scrollWidth<=e.clientWidth+1)).toBe(true);
  expect(await page.locator('.plugin-local-review-scroll').evaluate(e=>e.scrollHeight>e.clientHeight)).toBe(true);
  await inspect.click();
  await expect(page.getByText(/已核对安装材料/)).toBeVisible();
  await page.getByRole('button',{name:'返回',exact:true}).click();
  await expect(page.getByRole('button',{name:/本地/})).toBeEnabled();
});

for (const theme of ['light','dark']) test(`foreground task limitation stays visible at 320px with enlarged ${theme} text`, async ({ page }) => {
  await fixture(page);
  await page.evaluate(theme => {
    document.documentElement.classList.toggle('dark',theme==='dark');
    document.documentElement.style.fontSize='20px';
  },theme);
  const host=page.getByRole('status',{name:'原生插件宿主',exact:true});
  await expect(host).toContainText('后台执行尚未适配');
  await expect(host).toContainText('结果未确认的任务不会自动重试');
  expect(await host.evaluate(e=>e.scrollWidth<=e.clientWidth+1)).toBe(true);
  expect(await page.evaluate(()=>(window as any).mobilePlugin.calls.some((call:any)=>call.command==='plugins_sdk_call'))).toBe(false);
});
