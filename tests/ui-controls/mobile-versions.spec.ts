import { test, expect, type Page } from '@playwright/test';
import type { ReleaseScheduleSnapshot } from '../../src/lib/types/mobile-release';
type Call = { command: string; args?: Record<string, unknown> };
type Fixture = {
  __releaseCalls: Call[];
  __resolveOldCatalog?: () => void;
  __releaseWake: () => void;
  __changeReleasePreferences: () => void;
  __patchReleaseSchedule: (patch: Partial<ReleaseScheduleSnapshot>) => void;
  __holdSchedule: () => void;
  __releaseSchedule: () => void;
  __releaseSave: () => void;
  releaseNavigation: { back(): void; open(page: string): void; selectTab(tab: string): void };
};
async function calls(page: Page, command: string) { return page.evaluate(command => (window as unknown as Fixture).__releaseCalls.filter(call => call.command === command), command); }
async function openVersions(page: Page, scenario = '', kernel = false) {
  await page.setViewportSize({ width: 320, height: 740 });
  await page.addInitScript(({ scenario }) => {
    const target = window as unknown as Fixture & {
      __TAURI_INTERNALS__: { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>; transformCallback: (callback: (event: unknown) => void) => number };
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: (event: string, id: number) => void };
      __notesExecuted?: boolean;
    };
    target.__releaseCalls = [];
    let preferences = { clientAutoCheck: false, clientChannel: 'stable', kernelChannel: 'stable' };
    let revision = 'revision-before';
    let operation: { operationId: string; expectedRevision: string; preferences: typeof preferences; state: string; acknowledged: boolean; reviewBinding: string | null; reason: null; receipt: { ownerOperation: string; beforeEpoch: string; afterEpoch: string } | null } | null = null;
    target.__changeReleasePreferences = () => { preferences = {...preferences,clientAutoCheck:true};revision='revision-external'; };
    const snapshot = () => ({ preferences: { ...preferences }, expectedRevision: revision, schedulerState: operation && !operation.acknowledged ? 'preferences-unconfirmed' : scenario === 'desktop' ? 'not-native-host' : 'native-foreground', operation: operation && structuredClone(operation) });
    const catalog = (source: string, channel: string, status = 200) => ({
      source, channel, platform: 'android', checkedAtUnixMs: '1788697000000', catalogLimit: 30,
      requestId: `request-${source}-${channel}`, responseStatus: status, reusedValidatedCache: status === 304,
      targetStatus: source === 'kernel' ? 'bundled-via-client' : 'installation-not-adapted',
      targetReason: '目标安装适配尚未完成；目录版本不是兼容性确认。',
      releases: channel === 'test' ? [] : [{
        tagName: channel === 'stable' ? 'v1.0.1' : 'v1.1.0-rc.2', version: channel === 'stable' ? '1.0.1' : '1.1.0-rc.2', channel,
        kernelChannel: source === 'kernel' ? channel === 'stable' ? 'stable' : 'beta' : null,
        publishedAt: null, notes: '<img src=x onerror="window.__notesExecuted=true">原始发行说明', notesTruncated: false,
        releaseUrl: `https://github.com/zerodenet/${source === 'kernel' ? 'core' : 'znet-sink'}/releases/tag/v1.0.1`,
      }],
    });
    let schedule = {hostSessionId:'host-original',revision:'1',foreground:true,checking:false,enabled:false,clientChannel:'stable',lastAttemptedAtUnixMs:'1788697000000',lastSuccessAtUnixMs:'1788697000000',nextDueAtUnixMs:null,retryAtUnixMs:null,outcome:'disabled',reason:'自动检查已关闭。',storageError:false,cachedCatalog:catalog('client','stable'),cacheCurrent:scenario !== 'historical-cache',preferenceOperation:null,foregroundScheduling:'native-lifecycle',backgroundScheduling:'not-adapted'};
    if (scenario === 'no-cache') Object.assign(schedule, {cachedCatalog:null,cacheCurrent:false,lastAttemptedAtUnixMs:null,lastSuccessAtUnixMs:null});
    if (scenario === 'failure-cooldown') Object.assign(schedule,{outcome:'failed',reason:'上次检查失败，等待重试时间。',retryAtUnixMs:'1788697900000',cacheCurrent:false});
    target.__patchReleaseSchedule = patch => { schedule = { ...schedule, ...patch } as typeof schedule; };
    const callbacks = new Map<number, (event: unknown) => void>(); let callbackId = 0; let eventHandler = 0; let holdSchedule = false;
    target.__releaseWake = () => window.dispatchEvent(new CustomEvent('mobile-release:schedule-updated', {detail:{}}));
    target.__holdSchedule = () => { holdSchedule = true; };
    target.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => { eventHandler = 0; } };
    target.__TAURI_INTERNALS__ = {
      transformCallback: callback => { callbacks.set(++callbackId, callback); return callbackId; },
      invoke: async (command, args) => {
      target.__releaseCalls.push({ command, args });
      if (command === 'plugin:event|listen') { eventHandler = args!.handler as number; return 1; }
      if (command === 'plugin:event|unlisten') return null;
      if (command === 'plugin:app|version') return '1.0.0';
      if (command === 'mobile_app_update_status') return { state:'idle',operationId:null,reason:null,packageName:'org.zerodenet.znetsink.debug',installedVersion:'1.0.0',installedVersionCode:'1',targetVersion:null,targetVersionCode:null,downloadedBytes:'0',sizeBytes:null,assetName:null,sha256:null,installPermission:'allowed',confirmationAvailable:false,installerStatus:null };
      if (command === 'mobile_release_schedule') {
        const captured = structuredClone(schedule);
        if (holdSchedule) { holdSchedule = false; return new Promise(resolve => { target.__releaseSchedule = () => resolve(captured); }); }
        if (scenario === 'schedule-failed') throw {code:'unavailable',message:'token-secret'};
        return captured;
      }
      if (command === 'mobile_release_preferences') return snapshot();
      if (command === 'mobile_release_preferences_save') {
        if (args!.expectedRevision !== revision) throw {code:'conflict',message:'fixed CAS rejection'};
        operation = {operationId:args!.operationId as string,expectedRevision:args!.expectedRevision as string,preferences:args!.preferences as typeof preferences,state:'saved-awaiting-confirm',acknowledged:false,reviewBinding:'review-original',reason:null,receipt:{ownerOperation:args!.operationId as string,beforeEpoch:'1',afterEpoch:'2'}};
        preferences = {...operation.preferences}; revision = 'revision-after';
        Object.assign(schedule,{enabled:preferences.clientAutoCheck,clientChannel:preferences.clientChannel,revision:'2',reason:preferences.clientAutoCheck ? '等待宿主前台到期检查。' : '自动检查已关闭。',cachedCatalog:preferences.clientChannel==='stable'?schedule.cachedCatalog:null,cacheCurrent:false});
        if (scenario === 'unknown-save') throw { code: 'connection_closed', message: 'raw-token-secret' };
        if (scenario === 'hold-save') { const captured = snapshot(); return new Promise(resolve => { target.__releaseSave = () => resolve(captured); }); }
        return snapshot();
      }
      if (command === 'mobile_release_preferences_operation') {
        if (operation && operation.operationId === args!.operationId) return structuredClone(operation);
        return null;
      }
      if (command === 'mobile_release_preferences_confirm') {
        if (!operation || operation.operationId !== args!.operationId || operation.reviewBinding !== args!.reviewBinding) throw {code:'conflict'};
        operation = {...operation,state:'confirmed',acknowledged:true};
        if (scenario === 'unknown-confirm') throw {code:'connection_closed'};
        return snapshot();
      }
      if (command === 'mobile_release_catalog') {
        const source = args!.source as string; const channel = args!.channel as string;
        if (scenario === 'stale-channel' && channel === 'stable') return new Promise(resolve => { target.__resolveOldCatalog = () => resolve(catalog(source, channel)); });
        if (scenario === 'catalog-failed') throw new Error('https://secret.test/token');
        const result = catalog(source, channel, scenario === '304' ? 304 : 200);
        if(source==='client' && channel===schedule.clientChannel) Object.assign(schedule,{cachedCatalog:result,cacheCurrent:true});
        return result;
      }
      return null;
    } };
  }, { scenario });
  await page.goto(`/mobile-versions.html${kernel ? '?kernel' : ''}`);
  await expect(page.getByText('1.0.0', { exact: true })).toBeVisible();
}
test('real version page reads host cache without GET, renders notes as text and persists ordinary intent with owned acknowledgement', async ({ page }) => {
  await openVersions(page);
  await expect(page.getByRole('status').filter({ hasText: '发现较新的项目版本' })).toBeVisible();
  expect(await calls(page,'mobile_release_catalog')).toHaveLength(0);
  await expect(page.locator('.release-notes')).toContainText('<img src=x'); await expect(page.locator('.release-notes img')).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { __notesExecuted?: boolean }).__notesExecuted)).toBeUndefined();
  await page.getByRole('button', { name: '自动检查意图：关闭' }).click(); expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(0);
  await page.getByRole('button', { name: '保存发行偏好', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: '发行偏好已保存并确认' })).toBeVisible();
  const writes=await calls(page,'mobile_release_preferences_save');expect(writes).toHaveLength(1);expect(writes[0].args).toMatchObject({expectedRevision:'revision-before',preferences:{clientAutoCheck:true,clientChannel:'stable',kernelChannel:'stable'}});expect(writes[0].args!.operationId).toMatch(/^[0-9a-f]{32}$/);
  expect(await calls(page,'mobile_release_preferences_confirm')).toEqual([{command:'mobile_release_preferences_confirm',args:{operationId:writes[0].args!.operationId,reviewBinding:'review-original'}}]);
  await page.getByRole('radio', { name: '开发版', exact: true }).click(); await page.getByRole('button', { name: '检查当前渠道', exact:true }).click();
  await expect(page.getByRole('status').filter({ hasText: '暂无发行记录' })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBe(0);
  for (const button of await page.locator('button').all()) { const box=await button.boundingBox(); if(box) expect(box.height).toBeGreaterThanOrEqual(44); }
  expect((await page.evaluate(()=>(window as unknown as Fixture).__releaseCalls)).some(call=>/download|install|updater|core_start/.test(call.command))).toBe(false);
});
test('old pending channel cannot overwrite candidate result and failed catalog stays explicitly unknown', async ({ page }) => {
  await openVersions(page,'stale-channel');await page.getByRole('button',{name:'检查当前渠道',exact:true}).click();
  await expect.poll(async()=>(await calls(page,'mobile_release_catalog')).length).toBe(1);
  await page.getByRole('radio',{name:'候选版',exact:true}).click();await page.getByRole('button',{name:'检查当前渠道',exact:true}).click();
  await expect(page.getByRole('heading',{name:'1.1.0-rc.2 发行说明'})).toBeVisible();
  await page.evaluate(()=>(window as unknown as Fixture).__resolveOldCatalog?.());await expect(page.getByRole('heading',{name:'1.1.0-rc.2 发行说明'})).toBeVisible();await expect(page.getByRole('heading',{name:'1.0.1 发行说明'})).toHaveCount(0);
  const failed=await page.context().newPage();await openVersions(failed,'catalog-failed');await failed.getByRole('button',{name:'检查当前渠道',exact:true}).click();await expect(failed.getByRole('alert')).toContainText('无法确认是否有更新');await expect(failed.getByRole('alert')).not.toContainText('secret');await expect(failed.getByRole('status').filter({hasText:'未发现更新版本'})).toHaveCount(0);await failed.close();
});
test('lost preference save only reads original operation then requires explicit receipt confirmation; kernel is manual only', async ({ page }) => {
  await openVersions(page,'unknown-save');await page.getByRole('button',{name:'自动检查意图：关闭'}).click();await page.getByRole('button',{name:'保存发行偏好',exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('保存结果未确认');await expect(page.getByRole('button',{name:'放弃草稿并重新读取发行偏好',exact:true})).toBeDisabled();await expect(page.getByRole('button',{name:'保存发行偏好',exact:true})).toBeDisabled();
  await page.getByRole('button',{name:'核对原偏好操作',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'原操作已有本地持久回执'})).toBeVisible();expect(await calls(page,'mobile_release_preferences_confirm')).toHaveLength(0);
  await page.getByRole('button',{name:'确认原操作本地回执',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'已保存并确认'})).toBeVisible();expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(1);
  await openVersions(page,'',true);expect(await calls(page,'mobile_release_catalog')).toHaveLength(0);await page.getByRole('button',{name:'检查当前渠道',exact:true}).click();
  await expect(page.getByText('随包内核产物未接入',{exact:true})).toBeVisible();await expect(page.getByRole('status').filter({hasText:'当前版本未确认'})).toBeVisible();await expect(page.getByRole('button',{name:'通过客户端升级更新内核'})).toBeVisible();await expect(page.getByText('候选版对应内核 beta',{exact:false})).toBeVisible();await expect(page.getByRole('button',{name:/自动检查意图/})).toHaveCount(0);
});
test('saved channel facts stay separate from draft and foreground event only reads actual due state',async({page})=>{
  await openVersions(page);await page.getByRole('radio',{name:'候选版',exact:true}).click();const facts=page.getByRole('region',{name:'前台自动检查状态'});await expect(facts).toContainText('稳定版');expect(await calls(page,'mobile_release_catalog')).toHaveLength(0);
  await page.evaluate(()=>{const w=window as unknown as Fixture;w.__patchReleaseSchedule({revision:'2',enabled:true,lastAttemptedAtUnixMs:'1788698000000',lastSuccessAtUnixMs:'1788698000000',nextDueAtUnixMs:'1788719600000',outcome:'success',reason:'本次前台检查已完成。'});w.__releaseWake();});
  await expect(facts).toContainText('本次前台检查已完成');await expect(page.getByRole('radio',{name:'候选版',exact:true})).toBeChecked();await expect(page.getByRole('heading',{name:'1.0.1 发行说明'})).toHaveCount(0);expect(await calls(page,'mobile_release_catalog')).toHaveLength(0);expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(0);
});
test('manual 304 reuses validated catalog and no cache never means no update',async({page})=>{
  await openVersions(page,'304');await page.getByRole('button',{name:'检查当前渠道',exact:true}).click();await expect(page.getByText('服务器返回 304，已核对并复用原有效目录。',{exact:true})).toBeVisible();expect(await calls(page,'mobile_release_catalog')).toHaveLength(1);
  const empty=await page.context().newPage();await openVersions(empty,'no-cache');await expect(empty.getByRole('status').filter({hasText:/未发现更新|暂无发行记录/})).toHaveCount(0);expect(await calls(empty,'mobile_release_catalog')).toHaveLength(0);await expect(empty.getByRole('region',{name:'前台自动检查状态'})).toContainText('—');await empty.close();
});
test('failed schedule and failure cooldown preserve historical catalog without current success',async({page})=>{
  await openVersions(page,'failure-cooldown');await expect(page.getByText('上次成功目录（历史）',{exact:false})).toBeVisible();await expect(page.getByRole('region',{name:'前台自动检查状态'})).toContainText('等待重试时间');await expect(page.getByRole('status').filter({hasText:'发现较新'})).toHaveCount(0);
  const failed=await page.context().newPage();await openVersions(failed,'schedule-failed');await expect(failed.getByRole('alert')).toContainText('前台检查状态读取失败');await expect(failed.getByRole('alert')).not.toContainText('secret');expect(await calls(failed,'mobile_release_catalog')).toHaveLength(0);await failed.close();
});
test('unknown acknowledgement is not resent and actual owned status resolves it',async({page})=>{
  await openVersions(page,'unknown-confirm');await page.getByRole('button',{name:'自动检查意图：关闭'}).click();await page.getByRole('button',{name:'保存发行偏好',exact:true}).click();await expect(page.getByRole('alert')).toContainText('确认结果未确认');await expect(page.getByRole('button',{name:'确认原操作本地回执',exact:true})).toBeDisabled();
  await page.getByRole('button',{name:'检查当前渠道',exact:true}).click();await expect(page.getByRole('button',{name:'保存发行偏好',exact:true})).toBeDisabled();await page.getByRole('button',{name:'核对原偏好操作',exact:true}).click();await expect(page.getByRole('status').filter({hasText:'已保存并确认'})).toBeVisible();expect(await calls(page,'mobile_release_preferences_confirm')).toHaveLength(1);expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(1);
});
test('dirty back cancellation preserves draft and explicit discard leaves without save',async({page})=>{
  await openVersions(page);await page.getByRole('radio',{name:'候选版',exact:true}).click();await page.evaluate(()=>(window as unknown as Fixture).releaseNavigation.back());const dialog=page.getByRole('dialog',{name:'放弃未保存的设置？'});await expect(dialog).toBeVisible();await dialog.getByRole('button',{name:'继续编辑',exact:true}).click();await expect(page.getByRole('radio',{name:'候选版',exact:true})).toBeChecked();await page.evaluate(()=>(window as unknown as Fixture).releaseNavigation.back());await dialog.getByRole('button',{name:'放弃修改',exact:true}).click();await expect(page.getByText('更多',{exact:true})).toBeVisible();expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(0);
});
test('in-flight save blocks back and completion does not auto navigate',async({page})=>{
  await openVersions(page,'hold-save');await page.getByRole('button',{name:'自动检查意图：关闭'}).click();await page.getByRole('button',{name:'保存发行偏好',exact:true}).click();await expect.poll(async()=>(await calls(page,'mobile_release_preferences_save')).length).toBe(1);await expect(page.getByRole('button',{name:'放弃草稿并重新读取发行偏好',exact:true})).toBeDisabled();await page.evaluate(()=>(window as unknown as Fixture).releaseNavigation.back());const dialog=page.getByRole('dialog',{name:'设置正在提交'});await expect(dialog).toBeVisible();await dialog.getByRole('button',{name:'继续编辑',exact:true}).click();await page.evaluate(()=>(window as unknown as Fixture).__releaseSave());await expect(page.getByRole('status').filter({hasText:'已保存并确认'})).toBeVisible();await expect(page.getByRole('region',{name:'客户端升级管理'})).toBeVisible();expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(1);
});
test('wake during held read drops old current facts and fetches fresh snapshot, destroyed observer only releases its listener',async({page})=>{
  await openVersions(page);await page.evaluate(()=>(window as unknown as Fixture).__holdSchedule());await page.getByRole('button',{name:'重新读取前台状态',exact:true}).click();await page.evaluate(()=>{const w=window as unknown as Fixture;w.__patchReleaseSchedule({hostSessionId:'host-new',revision:'1',reason:'新宿主前台状态。',cachedCatalog:null,cacheCurrent:false,lastAttemptedAtUnixMs:null,lastSuccessAtUnixMs:null});w.__releaseWake();w.__releaseSchedule();});await expect(page.getByRole('region',{name:'前台自动检查状态'})).toContainText('新宿主前台状态');expect(await calls(page,'mobile_release_catalog')).toHaveLength(0);
  await page.evaluate(()=>(window as unknown as Fixture).releaseNavigation.back());await expect(page.getByText('更多',{exact:true})).toBeVisible();const before=(await calls(page,'mobile_release_schedule')).length;await page.evaluate(()=>(window as unknown as Fixture).__releaseWake());expect((await calls(page,'mobile_release_schedule')).length).toBe(before);expect((await page.evaluate(()=>(window as unknown as Fixture).__releaseCalls)).some(c=>/cancel|shutdown|stop/.test(c.command))).toBe(false);
});
test('manual catalog failure survives host wake with historical cache and desktop preview explains unsupported auto scheduler',async({page})=>{
  await openVersions(page,'catalog-failed');await page.getByRole('button',{name:'检查当前渠道',exact:true}).click();await expect(page.getByRole('alert')).toContainText('无法确认是否有更新');await page.evaluate(()=>{const w=window as unknown as Fixture;w.__patchReleaseSchedule({revision:'2',cacheCurrent:false,foregroundScheduling:'not-native-host',outcome:'failed'});w.__releaseWake();});await expect(page.getByRole('status').filter({hasText:'当前为桌面宿主'})).toBeVisible();await expect(page.getByRole('alert')).toContainText('无法确认是否有更新');await expect(page.getByText('上次成功目录（历史）',{exact:false})).toBeVisible();await expect(page.getByRole('status').filter({hasText:/未发现更新|发现较新/})).toHaveCount(0);expect(await calls(page,'mobile_release_catalog')).toHaveLength(1);
});
test('explicit discard and reload recovers an externally changed preference baseline without rebasing dirty or replaying writes',async({page})=>{
  await openVersions(page);
  await page.getByRole('radio',{name:'候选版',exact:true}).click();
  await page.evaluate(()=>(window as unknown as Fixture).__changeReleasePreferences());
  await page.getByRole('button',{name:'保存发行偏好',exact:true}).click();
  await expect(page.getByRole('alert')).toContainText('保存被明确拒绝');
  expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(1);
  const reads=(await calls(page,'mobile_release_preferences')).length;
  await page.evaluate(()=>document.dispatchEvent(new Event('visibilitychange')));
  await expect.poll(async()=>(await calls(page,'mobile_release_preferences')).length).toBeGreaterThan(reads);
  await expect(page.getByRole('radio',{name:'候选版',exact:true})).toBeChecked();
  const discard=page.getByRole('button',{name:'放弃草稿并重新读取发行偏好',exact:true});
  await discard.click();
  const dialog=page.getByRole('dialog',{name:'放弃发行偏好草稿并重新读取？'});
  await expect(dialog).toBeVisible();
  const modalBox=await dialog.boundingBox();expect(modalBox!.x).toBeGreaterThanOrEqual(0);expect(modalBox!.x+modalBox!.width).toBeLessThanOrEqual(320);
  for(const button of await dialog.getByRole('button').all()){const box=await button.boundingBox();if(box)expect(box.height).toBeGreaterThanOrEqual(44);}
  await dialog.getByRole('button',{name:'继续编辑',exact:true}).click();
  await expect(page.getByRole('radio',{name:'候选版',exact:true})).toBeChecked();
  expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(1);
  await discard.click();await dialog.getByRole('button',{name:'放弃草稿并读取',exact:true}).click();
  await expect(page.getByRole('radio',{name:'稳定版',exact:true})).toBeChecked();
  await expect(page.getByRole('button',{name:'自动检查意图：开启',exact:true})).toHaveAttribute('aria-pressed','true');
  expect(await calls(page,'mobile_release_preferences_save')).toHaveLength(1);expect(await calls(page,'mobile_release_preferences_confirm')).toHaveLength(0);
  await page.getByRole('radio',{name:'候选版',exact:true}).click();await page.getByRole('button',{name:'保存发行偏好',exact:true}).click();
  await expect(page.getByRole('status').filter({hasText:'已保存并确认'})).toBeVisible();
  const writes=await calls(page,'mobile_release_preferences_save');expect(writes).toHaveLength(2);expect(writes[1].args!.expectedRevision).toBe('revision-external');expect(writes[1].args!.operationId).not.toBe(writes[0].args!.operationId);
});
