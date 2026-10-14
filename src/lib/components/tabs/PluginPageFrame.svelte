<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    open as openFile,
    save as saveFile,
  } from '@tauri-apps/plugin-dialog';
  import { Button } from '$lib/components/ui/button';
  import {
    pluginApi,
    type PluginComponent,
    type PluginSnapshot,
  } from '$lib/services/plugins';
  import {
    PageChannel,
    PageChannelError,
    checkedDocument,
    componentFingerprint,
    publicFailure,
    boundedJson,
    PAGE_LIMITS,
    type PageDocument,
    type PageTicket,
  } from '$lib/features/plugin-pages/channel';
  import type { PluginNavigationRequest } from '$lib/services/plugin-navigation.svelte';

  let {
    pluginId,
    pageId,
    title,
    components,
    initialRoute,
    onSnapshot,
    api = pluginApi,
    chooseRead = () =>
      openFile({
        title: '选择插件要读取的文件',
        multiple: false,
        directory: false,
      }),
    chooseWrite = () => saveFile({ title: '选择插件文件保存位置' }),
  }: {
    pluginId: string;
    pageId: string;
    title: string;
    components: PluginComponent[];
    initialRoute: PluginNavigationRequest | null;
    onSnapshot: (snapshot: PluginSnapshot) => void;
    api?: Pick<
      typeof pluginApi,
      | 'pageDocument'
      | 'pageClose'
      | 'pageCheck'
      | 'pageConfigure'
      | 'pageInvoke'
      | 'pageSdk'
      | 'pageProtectedLoad'
      | 'pagePreflight'
      | 'snapshot'
    >;
    chooseRead?: () => Promise<string | string[] | null>;
    chooseWrite?: () => Promise<string | null>;
  } = $props();

  let frame = $state<HTMLIFrameElement>();
  let pageDocument = $state('');
  let loading = $state(true);
  let failure = $state('');
  let generation = 0;
  let channel = '';
  let documentBinding = $state.raw<PageDocument | null>(null);
  let frameKey = $state(0);
  let mounted = $state(false);
  let visible = $state(true);
  let watched = '';
  let attachedKey = -1;
  const bridge = new PageChannel();
  const host = untrack(() => api);
  const readPicker = untrack(() => chooseRead),
    writePicker = untrack(() => chooseWrite);
  function closeDocument(review: string) {
    void host.pageClose(review).catch(() => undefined);
  }
  function retire(reason = '') {
    generation++;
    bridge.close();
    if (documentBinding) closeDocument(documentBinding.document_review);
    documentBinding = null;
    pageDocument = '';
    loading = false;
    failure = reason;
  }
  function assertCurrent(ticket: PageTicket) {
    if (!bridge.currentTicket(ticket)) throw new PageChannelError('stale');
  }
  function scriptJson(value: unknown) {
    return JSON.stringify(value).replace(
      /[<>&\u2028\u2029]/g,
      (c) => '\\u' + c.charCodeAt(0).toString(16).padStart(4, '0'),
    );
  }

  const themeTokens = [
    ['--background', '#f2f2f5'],
    ['--foreground', '#1a1a1e'],
    ['--card', '#ffffff'],
    ['--border', 'rgba(0,0,0,.055)'],
    ['--surface', 'rgba(255,255,255,.65)'],
    ['--primary', '#18181b'],
    ['--primary-foreground', '#fafafa'],
    ['--secondary', '#e7e7ec'],
    ['--secondary-foreground', '#27272a'],
    ['--input', 'rgba(24,24,27,.14)'],
    ['--ring', '#2563eb'],
    ['--muted', 'rgba(0,0,0,.04)'],
    ['--muted-foreground', '#78788c'],
    ['--accent', 'rgba(59,130,246,.08)'],
    ['--accent-foreground', '#1d4ed8'],
    ['--destructive', '#ef4444'],
    ['--success', '#22c55e'],
    ['--warning', '#f59e0b'],
    ['--font-sans', 'Inter,system-ui,sans-serif'],
  ] as const;

  function sdk(
    channelId: string,
    id: string,
    navigation: PluginNavigationRequest | null,
  ) {
    const plugin = scriptJson(id),
      channelValue = scriptJson(channelId);
    const navigationValue = scriptJson(
      navigation
        ? {
            route: navigation.route,
            ...(navigation.reference
              ? { reference: navigation.reference }
              : {}),
          }
        : null,
    );
    return `<script>
(() => {
  const channel = ${channelValue};
  const pluginId = ${plugin};
  const initialNavigation = ${navigationValue};
  let nextId = 1;
  let readyResolve; const ready=new Promise(resolve=>readyResolve=resolve);
  const pending = new Map();
  function request(method, args = {}) {
    return ready.then(()=>new Promise((resolve, reject) => {
      if(pending.size>=8||nextId>512){const error=new Error('插件页面请求超过客户端预算');error.code='budget_exceeded';reject(error);return;}
      const id = nextId++;
      pending.set(id, { resolve, reject });
      parent.postMessage({ source: 'znet-plugin-page-v1', channel, id, method, args }, '*');
    }));
  }
  addEventListener('message', event => {
    const value = event.data;
    if(event.source!==parent||!value||value.source!=='znet-plugin-host-v1'||value.channel!==channel)return;
    if(value.id===0&&value.ok){readyResolve();return;}
    const item = pending.get(value.id);
    if (!item) return;
    pending.delete(value.id);
    if (value.ok) {
      item.resolve(value.value);
      return;
    }
    const failure = value.error && typeof value.error === 'object'
      ? value.error
      : { code: 'host_operation_failed', message: String(value.error || '插件宿主操作失败') };
    const error = new Error(failure.message || '插件宿主操作失败');
    Object.assign(error, {
      code: failure.code,
      fieldPath: failure.field_path,
      diagnostics: Array.isArray(failure.diagnostics) ? failure.diagnostics : [],
      retryAfterMs: failure.retry_after_ms
    });
    item.reject(error);
  });
  const utf8 = {
    encode(value) { const bytes = new TextEncoder().encode(JSON.stringify(value)); let binary = ''; for (const byte of bytes) binary += String.fromCharCode(byte); return btoa(binary); },
    decode(value) { const binary = atob(value); const bytes = Uint8Array.from(binary, char => char.charCodeAt(0)); return JSON.parse(new TextDecoder().decode(bytes)); }
  };
  async function sdkCall(componentId, capability, scope, method, argumentsValue = {}, budget) {
    const call = { version: 1, request: { capability, scope }, method, arguments: argumentsValue };
    if (budget) call.budget = budget;
    const reply = await request('sdk.call', { componentId, call });
    if (!reply || reply.version !== 1 || !reply.ok) {
      const error = new Error(reply && reply.error ? reply.error.message : '插件 SDK 操作失败');
      if (reply && reply.error) Object.assign(error, {
        code: reply.error.code,
        fieldPath: reply.error.field_path,
        diagnostics: Array.isArray(reply.error.diagnostics) ? reply.error.diagnostics : [],
        retryAfterMs: reply.error.retry_after_ms
      });
      throw error;
    }
    return reply.value;
  }
  Object.defineProperty(globalThis, 'znetPlugin', { value: Object.freeze({
    sdkVersion: 1,
    pluginId,
    context: () => request('context'),
    configuration: Object.freeze({
      get: componentId => request('configuration.get', { componentId }),
      save: (componentId, values) => request('configuration.save', { componentId, values })
    }),
    navigation: Object.freeze({ initial: () => initialNavigation }),
    storage: Object.freeze({
      get: (componentId, area, key) => sdkCall(componentId, 'plugin.storage.read', 'self', 'storage_get', { area, key }),
      put: (componentId, area, key, value) => sdkCall(componentId, 'plugin.storage.write', 'self', 'storage_put', { area, key, value }),
      delete: (componentId, area, key) => sdkCall(componentId, 'plugin.storage.write', 'self', 'storage_delete', { area, key }),
      getJson: async (componentId, area, key) => { const value = await sdkCall(componentId, 'plugin.storage.read', 'self', 'storage_get', { area, key }); return value == null ? null : utf8.decode(value); },
      putJson: (componentId, area, key, value) => sdkCall(componentId, 'plugin.storage.write', 'self', 'storage_put', { area, key, value: utf8.encode(value) }),
      list: (componentId, area) => sdkCall(componentId, 'plugin.storage.read', 'self', 'storage_list', { area }),
      export: (componentId, area) => sdkCall(componentId, 'plugin.storage.read', 'self', 'storage_export', { area }),
      clear: (componentId, area) => sdkCall(componentId, 'plugin.storage.write', 'self', 'storage_clear', { area }),
      migrate: (componentId, area, from, to, values) => sdkCall(componentId, 'plugin.storage.write', 'self', 'storage_migrate', { area, from, to, values })
    }),
    logs: Object.freeze({
      write: (componentId, level, message, fields = null) => sdkCall(componentId, 'plugin.logs.write', 'self', 'log_write', { level, message, fields })
    }),
    notifications: Object.freeze({
      post: (componentId, message, options = {}) => sdkCall(componentId, 'notifications.post', 'self', 'notification_post', { message, ...options })
    }),
    tasks: Object.freeze({
      put: (componentId, taskId, action, intervalSeconds) => sdkCall(componentId, 'tasks.schedule', 'self', 'schedule_put', { taskId, action, intervalSeconds }),
      list: componentId => sdkCall(componentId, 'tasks.schedule', 'self', 'schedule_list'),
      delete: (componentId, taskId) => sdkCall(componentId, 'tasks.schedule', 'self', 'schedule_delete', { taskId })
    }),
    subscriptions: Object.freeze({
      completeSync: (componentId, providerId, remoteSubscriptionId, revision) => sdkCall(
        componentId, 'subscriptions.manage', 'self', 'subscription_sync_complete',
        { providerId, remoteSubscriptionId, revision }
      ),
      updateMetadata: (componentId, providerId, remoteSubscriptionId, usage) => sdkCall(
        componentId,
        'subscriptions.manage',
        'self',
        'subscription_metadata_update',
        { providerId, remoteSubscriptionId, usage }
      )
    }),
    browser: Object.freeze({
      open: (componentId, origin, url) => sdkCall(componentId, 'browser.open', origin, 'browser_open', { url }),
      createCallback: componentId => sdkCall(componentId, 'browser.callback', 'self', 'callback_create'),
      pollCallback: (componentId, sessionId) => sdkCall(componentId, 'browser.callback', 'self', 'callback_poll', { sessionId }),
      cancelCallback: (componentId, sessionId) => sdkCall(componentId, 'browser.callback', 'self', 'callback_cancel', { sessionId })
    }),
    files: Object.freeze({
      read: (componentId, options = {}) => sdkCall(componentId, 'files.selection.read', 'user', 'file_read', options),
      write: (componentId, dataBase64) => sdkCall(componentId, 'files.selection.write', 'user', 'file_write', { dataBase64 })
    }),
    materials: Object.freeze({
      submit: (componentId, dataBase64, options = {}) => sdkCall(componentId, 'materials.submit', 'configuration', 'material_submit', { dataBase64, ...options }),
      drop: (componentId, handle) => sdkCall(componentId, 'materials.submit', 'configuration', 'material_drop', { handle }),
      receiveSecret: (componentId, origin, options) => sdkCall(componentId, 'secrets.session.receive', origin, 'secret_receive', options),
      crypto: (componentId, argumentsValue) => sdkCall(componentId, 'crypto.session.use', 'self', 'crypto_use', argumentsValue),
      loadRuntime: (componentId, handle) => sdkCall(componentId, 'runtime.protected.load', 'active-runtime', 'protected_load', { handle })
    }),
    capabilities: Object.freeze({ call: sdkCall }),
    invoke: (componentId, action, payload = null) => request('component.invoke', { componentId, action, payload })
  }), writable: false, configurable: false });
  addEventListener('pagehide',()=>parent.postMessage({source:'znet-plugin-page-v1',channel,id:0,method:'closing',args:{}},'*'));
  parent.postMessage({ source: 'znet-plugin-page-v1', channel, id: 0, method: 'ready', args: {} }, '*');
})();
<\/script>`;
  }

  function hostUi() {
    const computed = getComputedStyle(document.documentElement);
    const tokens = themeTokens
      .map(([name, fallback]) => {
        const value =
          computed
            .getPropertyValue(name)
            .trim()
            .replace(/[<>{};]/g, '') || fallback;
        return `${name}:${value}`;
      })
      .join(';');
    const scheme = document.documentElement.classList.contains('dark')
      ? 'dark'
      : 'light';
    return `<style data-znet-plugin-ui="1">
:root{${tokens};color-scheme:${scheme};font-family:var(--font-sans)}
*{box-sizing:border-box}html,body{width:100%;height:100%;margin:0;overflow:hidden}body{background:var(--background);color:var(--foreground);font:12px/1.55 var(--font-sans);-webkit-font-smoothing:antialiased}button,input{font:inherit}button{cursor:pointer}button:focus-visible,input:focus-visible{outline:2px solid var(--ring);outline-offset:2px}
[hidden]{display:none!important}[data-znet-layout="settings"]{display:flex;width:100%;height:100%;overflow:hidden;background:var(--card)}
[data-znet-settings-nav]{display:flex;width:min(156px,30vw);min-width:112px;flex-shrink:0;flex-direction:column;gap:1px;padding:14px 8px;border-right:1px solid var(--border);background:var(--surface)}
[data-znet-nav-title]{padding:4px 8px 10px;color:var(--muted-foreground);font-size:10.5px;font-weight:700;letter-spacing:.08em;text-transform:uppercase;opacity:.65}
[data-znet-nav-group]{margin:8px 8px 3px;color:var(--muted-foreground);font-size:9.5px;font-weight:700;letter-spacing:.06em;text-transform:uppercase;opacity:.58}
[data-znet-settings-item]{display:flex;align-items:center;gap:8px;width:100%;padding:7px 10px;border:0;border-radius:6px;background:transparent;color:var(--muted-foreground);font-weight:500;text-align:left;transition:background .13s ease,color .13s ease}
[data-znet-settings-item]:hover:not(:disabled){background:var(--muted);color:var(--foreground)}[data-znet-settings-item][aria-current="step"]{background:var(--primary);color:var(--primary-foreground);font-weight:600}[data-znet-settings-item]:disabled{cursor:not-allowed;opacity:.44}
[data-znet-step-mark]{display:grid;width:16px;height:16px;flex:0 0 16px;place-items:center;border:1px solid currentColor;border-radius:50%;font-size:9px;line-height:1}[data-znet-settings-item][data-complete="true"] [data-znet-step-mark]{background:var(--success);border-color:var(--success);color:white}
[data-znet-settings-content]{flex:1;min-width:0;min-height:0;overflow:auto;padding:18px 20px 24px}[data-znet-page-header]{max-width:720px;margin:0 auto 18px;padding-bottom:14px;border-bottom:1px solid var(--border)}[data-znet-page-header] h1{margin:0;font-size:18px;font-weight:650;letter-spacing:-.01em}[data-znet-page-header] p{margin:5px 0 0;color:var(--muted-foreground)}
[data-znet-view]{display:grid;max-width:720px;margin:0 auto;gap:14px}[data-znet-panel]{display:grid;gap:14px;padding:16px;border:1px solid var(--border);border-radius:10px;background:var(--card);box-shadow:0 1px 2px rgb(0 0 0 / 4%)}[data-znet-panel-header] h2{margin:0;font-size:13px;font-weight:650}[data-znet-panel-header] p{margin:4px 0 0;color:var(--muted-foreground);font-size:11px}
[data-znet-fields]{display:grid;gap:12px}[data-znet-field]{display:grid;gap:5px}[data-znet-field]>span{font-size:11px;font-weight:600}[data-znet-field]>small{color:var(--muted-foreground);font-size:10.5px}input{width:100%;height:30px;padding:0 10px;border:1px solid var(--input);border-radius:7px;background:var(--background);color:var(--foreground)}input::placeholder{color:var(--muted-foreground)}
[data-znet-select]{position:relative;width:100%}[data-znet-select-trigger]{display:flex;width:100%;height:30px;align-items:center;justify-content:space-between;gap:10px;padding:0 9px 0 10px;border:1px solid var(--input);border-radius:7px;background:var(--background);color:var(--foreground);text-align:left;transition:border-color .13s ease,background .13s ease,box-shadow .13s ease}[data-znet-select-trigger]:hover{border-color:color-mix(in srgb,var(--foreground) 24%,var(--input));background:var(--card)}[data-znet-select-trigger][aria-expanded="true"]{border-color:var(--ring);box-shadow:0 0 0 2px color-mix(in srgb,var(--ring) 18%,transparent)}[data-znet-select-icon]{width:14px;height:14px;flex:0 0 14px;color:var(--muted-foreground);transition:transform .13s ease}[data-znet-select-trigger][aria-expanded="true"] [data-znet-select-icon]{transform:rotate(180deg)}
[data-znet-select-content]{position:absolute;z-index:30;top:calc(100% + 5px);left:0;width:100%;min-width:160px;padding:4px;border:1px solid var(--border);border-radius:8px;background:var(--card);box-shadow:0 10px 30px rgb(0 0 0 / 14%)}[data-znet-select-option]{display:flex;width:100%;min-height:30px;align-items:center;justify-content:space-between;gap:12px;padding:6px 8px;border:0;border-radius:6px;background:transparent;color:var(--foreground);text-align:left}[data-znet-select-option]:hover,[data-znet-select-option]:focus-visible{background:var(--muted);outline:0}[data-znet-select-option][aria-selected="true"]{background:var(--accent);color:var(--accent-foreground);font-weight:600}[data-znet-select-option][aria-selected="true"]::after{content:'\\2713';font-size:11px}
[data-znet-actions]{display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap;padding-top:2px}button[data-variant]{min-height:30px;padding:0 12px;border:1px solid transparent;border-radius:7px;font-weight:550}button[data-variant="primary"]{background:var(--primary);color:var(--primary-foreground)}button[data-variant="outline"]{border-color:var(--input);background:var(--background);color:var(--foreground)}button[data-variant="ghost"]{background:transparent;color:var(--muted-foreground)}button[data-variant]:disabled{cursor:not-allowed;opacity:.45}
[data-znet-notice]{max-width:720px;margin:0 auto 14px;padding:9px 11px;border:1px solid var(--border);border-radius:8px;background:var(--muted);color:var(--muted-foreground)}[data-znet-notice="error"]{border-color:color-mix(in srgb,var(--destructive) 35%,var(--border));color:var(--destructive)}[data-znet-notice="success"]{border-color:color-mix(in srgb,var(--success) 35%,var(--border));color:var(--success)}
[data-znet-status-list]{display:grid;gap:0}[data-znet-status]{display:grid;grid-template-columns:16px minmax(0,1fr);gap:9px;padding:10px 0;border-bottom:1px solid var(--border)}[data-znet-status]:last-child{border-bottom:0}[data-znet-status-mark]{width:8px;height:8px;margin:5px 0 0 3px;border-radius:50%;background:var(--muted-foreground)}[data-znet-status="ready"] [data-znet-status-mark]{background:var(--success)}[data-znet-status="blocked"] [data-znet-status-mark],[data-znet-status="action_required"] [data-znet-status-mark]{background:var(--warning)}[data-znet-status] strong{display:block;font-size:11.5px}[data-znet-status] small{display:block;margin-top:2px;color:var(--muted-foreground)}
[data-znet-callout]{padding:11px 12px;border-radius:8px;background:var(--muted);color:var(--muted-foreground)}[data-znet-callout] strong{display:block;color:var(--foreground);margin-bottom:3px}[data-znet-summary]{display:grid;gap:8px;margin:0}[data-znet-summary]>div{display:grid;grid-template-columns:120px minmax(0,1fr);gap:12px;padding:7px 0;border-bottom:1px solid var(--border)}[data-znet-summary] dt{color:var(--muted-foreground)}[data-znet-summary] dd{margin:0;overflow-wrap:anywhere}pre[data-znet-code]{max-height:260px;margin:0;overflow:auto;padding:12px;border-radius:8px;background:var(--muted);color:var(--muted-foreground);font:10.5px/1.55 ui-monospace,monospace;white-space:pre-wrap;overflow-wrap:anywhere}
@media(max-width:640px){[data-znet-layout="settings"]{flex-direction:column}[data-znet-settings-nav]{width:100%;min-width:0;max-height:138px;flex-flow:row wrap;align-content:flex-start;overflow:auto;border-right:0;border-bottom:1px solid var(--border);padding:8px}[data-znet-nav-title],[data-znet-nav-group]{display:none}[data-znet-settings-item]{width:auto;padding:6px 9px}[data-znet-settings-content]{padding:14px}[data-znet-summary]>div{grid-template-columns:90px minmax(0,1fr)}}
</style>`;
  }

  function wrap(
    html: string,
    channelId: string,
    id: string,
    navigation: PluginNavigationRequest | null,
  ) {
    const head = `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; font-src data:; style-src 'unsafe-inline' data:; script-src 'unsafe-inline' data:; connect-src 'none'; media-src data:; object-src 'none'; base-uri 'none'; form-action 'none'">${hostUi()}${sdk(channelId, id, navigation)}`;
    // Guest HTML may contain pre-head scripts, malformed markup or a fake head
    // inside a comment. Never parse any guest bytes before our real CSP/bootstrap.
    return `<!doctype html><html><head><meta charset="utf-8">${head}</head><body>${html}</body></html>`;
  }

  async function load() {
    const id = untrack(() => pluginId),
      page = untrack(() => pageId),
      navigation = untrack(() => initialRoute);
    watched = untrack(() =>
      JSON.stringify([
        pluginId,
        pageId,
        initialRoute?.token,
        componentFingerprint(components),
      ]),
    );
    retire();
    const current = generation;
    loading = true;
    try {
      const raw = await host.pageDocument(id, page);
      if (current !== generation || !visible || !mounted) {
        closeDocument(raw.document_review);
        return;
      }
      let doc: PageDocument;
      try {
        doc = checkedDocument(raw, id, page);
      } catch (reason) {
        if (
          typeof raw.document_review === 'string' &&
          raw.document_review.length <= 512
        )
          closeDocument(raw.document_review);
        throw reason;
      }
      documentBinding = doc;
      channel = crypto.randomUUID();
      frameKey++;
      attachedKey = -1;
      pageDocument = wrap(
        doc.html,
        channel,
        id,
        navigation?.pluginId === id && navigation.pageId === page
          ? navigation
          : null,
      );
    } catch (reason) {
      if (current === generation) failure = publicFailure(reason).message;
    } finally {
      if (current === generation) loading = false;
    }
  }
  function attach() {
    if (!frame?.contentWindow || !documentBinding) return;
    if (attachedKey === frameKey) {
      retire('插件页面发生导航，原消息通道已关闭，请重新打开');
      return;
    }
    attachedKey = frameKey;
    bridge.open(documentBinding, channel, frame.contentWindow);
    bridge.ready(frame.contentWindow, {
      source: 'znet-plugin-page-v1',
      channel,
      id: 0,
      method: 'ready',
      args: {},
    });
  }
  async function check(ticket: PageTicket) {
    assertCurrent(ticket);
    try {
      await host.pageCheck(ticket.epoch.document.document_review);
    } catch {
      if (bridge.currentTicket(ticket))
        retire('插件页面身份或授权无法确认，原通道已关闭，请重新检查');
      throw new PageChannelError('stale');
    }
    assertCurrent(ticket);
  }
  async function dispatch(ticket: PageTicket) {
    await check(ticket);
    const { method, args } = ticket;
    const doc = ticket.epoch.document;
    if (method === 'context')
      return {
        pluginId: doc.plugin_id,
        components: doc.components.map((c) => ({
          id: c.component_id,
          version: c.version,
          enabled: c.enabled,
          running: c.running,
          blocked: c.blocked ? '组件当前不可执行，请查看宿主状态' : null,
        })),
      };
    const selected = bridge.component(ticket),
      review = bridge.review(ticket);
    if (method === 'configuration.get')
      return selected.configuration?.values ?? {};
    if (method === 'configuration.save') {
      if (
        !args.values ||
        typeof args.values !== 'object' ||
        Array.isArray(args.values) ||
        Object.values(args.values).some((v) => typeof v !== 'string')
      )
        throw new PageChannelError('invalid_request');
      const snapshot = await host.pageConfigure(
        doc.document_review,
        review,
        args.values as Record<string, string>,
      );
      assertCurrent(ticket);
      onSnapshot(snapshot);
      if (
        componentFingerprint(
          snapshot.components.filter((c) => c.plugin_id === doc.plugin_id),
        ) !== componentFingerprint(doc.components)
      )
        retire('配置已保存，页面身份已变化，请重新打开管理页面');
      return true;
    }
    if (method !== 'component.invoke' && method !== 'sdk.call')
      throw new PageChannelError('unsupported');
    if (
      !selected.enabled ||
      selected.permission_review_required ||
      selected.blocked
    )
      throw new PageChannelError('permission_denied');
    if (method === 'component.invoke') {
      if (
        typeof args.action !== 'string' ||
        !args.action ||
        args.action.length > 128
      )
        throw new PageChannelError('invalid_request');
      const result = await host.pageInvoke(
        doc.document_review,
        review,
        args.action,
        args.payload ?? null,
      );
      assertCurrent(ticket);
      return result;
    }
    if (!args.call || typeof args.call !== 'object' || Array.isArray(args.call))
      throw new PageChannelError('invalid_request');
    const call = boundedJson(args.call, PAGE_LIMITS.requestBytes) as Parameters<
      typeof pluginApi.sdk
    >[2];
    if (
      call.version !== 1 ||
      !call.request ||
      typeof call.request.capability !== 'string' ||
      typeof call.request.scope !== 'string' ||
      typeof call.method !== 'string'
    )
      throw new PageChannelError('invalid_request');
    if (call.method === 'file_read' || call.method === 'file_write') {
      const values =
        call.arguments &&
        typeof call.arguments === 'object' &&
        !Array.isArray(call.arguments)
          ? { ...(call.arguments as Record<string, unknown>) }
          : {};
      delete values.selectedPath;
      call.arguments = values;
      const preflight = await host.pagePreflight(
        doc.document_review,
        review,
        call,
      );
      assertCurrent(ticket);
      if (!preflight.ok)
        return { version: 1, ok: false, error: publicFailure(preflight.error) };
      if (
        preflight.version !== 1 ||
        preflight.value?.state !== 'permission-checked'
      )
        throw new PageChannelError('stale');
      const path =
        call.method === 'file_read' ? await readPicker() : await writePicker();
      assertCurrent(ticket);
      if (typeof path !== 'string')
        return {
          version: 1,
          ok: false,
          error: publicFailure(new PageChannelError('cancelled')),
        };
      await check(ticket);
      call.arguments = { ...values, selectedPath: path };
    }
    const reply =
      call.method === 'protected_load'
        ? await host.pageProtectedLoad(doc.document_review, review, call)
        : await host.pageSdk(doc.document_review, review, call);
    assertCurrent(ticket);
    if (reply.version !== 1 || typeof reply.ok !== 'boolean')
      throw new PageChannelError('invalid_request');
    return reply.ok
      ? reply
      : { version: 1, ok: false, error: publicFailure(reply.error) };
  }
  onMount(() => {
    visible = !document.hidden;
    mounted = true;
    const receive = (event: MessageEvent) => {
      if (event.origin !== 'null') return;
      const data = event.data;
      if (
        event.source === bridge.current?.target &&
        data?.source === 'znet-plugin-page-v1' &&
        data.channel === bridge.current.channel &&
        data.id === 0 &&
        data.method === 'closing'
      ) {
        retire('插件页面已离开，原通道关闭');
        return;
      }
      let ticket: PageTicket | null = null;
      try {
        if (bridge.ready(event.source, data)) return;
        ticket = bridge.begin(event.source, data);
      } catch (reason) {
        if (!bridge.reject(event.source, data, reason))
          retire(publicFailure(reason).message);
        return;
      }
      if (!ticket) return;
      const request = ticket;
      void dispatch(request)
        .then((result) => {
          const delivered = bridge.result(request, result);
          if (!delivered && bridge.currentTicket(request))
            retire('插件页面交付预算已用尽，请重新打开');
          bridge.finish(request, delivered);
        })
        .catch((reason) => {
          const delivered = bridge.result(request, null, reason);
          if (!delivered && bridge.currentTicket(request))
            retire('插件页面交付预算已用尽，请重新打开');
          bridge.finish(request, delivered);
        });
    };
    const recover = async () => {
      retire();
      if (document.hidden) {
        visible = false;
        return;
      }
      visible = true;
      const current = generation;
      try {
        const snapshot = await host.snapshot();
        if (current !== generation || !mounted || document.hidden) return;
        onSnapshot(snapshot);
        await load();
      } catch {
        if (current === generation) failure = '插件状态无法核对，请重新检查';
      }
    };
    const visibility = () => {
      if (document.hidden) {
        visible = false;
        retire();
      } else void recover();
    };
    const pagehide = () => {
      visible = false;
      retire();
    };
    const pageshow = (event: PageTransitionEvent) => {
      if (event.persisted) visibility();
    };
    addEventListener('message', receive);
    document.addEventListener('visibilitychange', visibility);
    addEventListener('pagehide', pagehide);
    addEventListener('pageshow', pageshow);
    return () => {
      mounted = false;
      retire();
      removeEventListener('message', receive);
      document.removeEventListener('visibilitychange', visibility);
      removeEventListener('pagehide', pagehide);
      removeEventListener('pageshow', pageshow);
    };
  });
  $effect(() => {
    const key = JSON.stringify([
      pluginId,
      pageId,
      initialRoute?.token,
      componentFingerprint(components),
    ]);
    if (mounted && visible && key !== watched) {
      watched = key;
      void untrack(() => load());
    }
  });
</script>

<div class="plugin-page-frame">
  {#if loading}<p class="plugin-page-state">正在加载插件管理页面…</p>{/if}
  {#if failure}<div class="plugin-page-state error" role="alert">
      <p>{failure}</p>
      <Button size="sm" variant="outline" onclick={load}>重试</Button>
    </div>{/if}
  {#if documentBinding}<div class="plugin-page-source">
      插件页面 · {documentBinding.plugin_id} · {documentBinding.components
        .map((c) => c.version)
        .filter((v, i, a) => a.indexOf(v) === i)
        .join(' / ')}
    </div>{/if}
  {#if pageDocument && !failure}{#key frameKey}<iframe
        bind:this={frame}
        srcdoc={pageDocument}
        sandbox="allow-scripts"
        {title}
        onload={attach}
      ></iframe>{/key}{/if}
</div>

<style>
  .plugin-page-frame {
    display: flex;
    flex-direction: column;
    flex: 1;
    width: 100%;
    min-width: 0;
    min-height: 0;
    height: 100%;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--background);
  }
  .plugin-page-source {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    overflow-wrap: anywhere;
    color: var(--muted-foreground);
  }
  iframe {
    flex: 1;
    min-height: 0;
    display: block;
    width: 100%;
    height: 100%;
    border: 0;
    background: transparent;
  }
  .plugin-page-state {
    display: grid;
    width: 100%;
    height: 100%;
    min-height: 220px;
    place-items: center;
    margin: 0;
    color: var(--muted-foreground);
  }
  .plugin-page-state.error {
    align-content: center;
    gap: 10px;
    color: var(--destructive);
  }
  .plugin-page-state.error p {
    margin: 0;
  }
</style>
