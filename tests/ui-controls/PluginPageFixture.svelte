<script lang="ts">
  import PluginPageFrame from '$lib/components/tabs/PluginPageFrame.svelte';
  import { Button } from '$lib/components/ui/button';
  import {
    pluginApi,
    type PluginComponent,
    type PluginReview,
    type PluginSdkCall,
    type PluginSdkReply,
  } from '$lib/services/plugins';
  import {
    componentFingerprint,
    type PageDocument,
  } from '$lib/features/plugin-pages/channel';
  const initial: PluginComponent = {
    plugin_id: 'fixture.plugin',
    component_id: 'worker',
    name: 'Fixture',
    version: '1.0.0',
    publisher: 'Fixture publisher',
    review: {
      key: 'fixture.plugin/worker',
      identity: 'signed-fixture-v1',
      registration: 1,
      revision: 0,
    },
    enabled: true,
    running: false,
    blocked: null,
    permissions: [
      {
        request: { capability: 'files.selection.read', scope: 'user' },
        required: false,
        supported: true,
        granted: true,
      },
    ],
    configuration: {
      schema: { title: 'Fixture configuration', fields: [] },
      values: { name: 'signed' },
      configured: true,
    },
  };
  let authoritative = structuredClone(initial);
  let components = $state([structuredClone(initial)]);
  let route = $state(null);
  let documents = $state(0),
    closes = $state(0),
    invokes = $state(0),
    sdkCalls = $state(0),
    preflights = $state(0),
    pickers = $state(0),
    snapshots = $state(0);
  const docs = new Map<string, { fingerprint: string; closed: boolean }>();
  let pendingInvoke: ((result: unknown) => void) | null = null;
  let pendingPicker: ((path: string | null) => void) | null = null;
  const html = `<!doctype html><html><body><h1>Signed fixture page</h1><output id="result">ready</output><button id="unknown">Unknown component</button><button id="normal">Current operation</button><button id="late">Delayed operation</button><button id="file">Choose file</button><button id="browser">Open browser</button><button id="nav">Navigate</button><button id="huge">Oversized result</button><script>
  const out=document.getElementById('result');const run=task=>task().then(value=>out.textContent=JSON.stringify(value)).catch(error=>out.textContent=error.code+':'+error.message);
  document.getElementById('unknown').onclick=()=>run(()=>znetPlugin.invoke('unknown','normal'));
  document.getElementById('normal').onclick=()=>run(()=>znetPlugin.invoke('worker','normal'));
  document.getElementById('late').onclick=()=>run(()=>znetPlugin.invoke('worker','late'));
  document.getElementById('file').onclick=()=>run(()=>znetPlugin.files.read('worker',{selectedPath:'/guest/forged'}));
  document.getElementById('browser').onclick=()=>run(()=>znetPlugin.browser.open('worker','https://fixture.invalid','https://fixture.invalid/?token=SECRET'));
  document.getElementById('nav').onclick=()=>location.href='https://navigation.invalid/should-not-load?token=SECRET';
  document.getElementById('huge').onclick=()=>run(()=>znetPlugin.invoke('worker','huge'));
  <\/script></body></html>`;
  const documentMode = new URLSearchParams(location.search).get(
    'document-mode',
  );
  const probeOrigin =
    new URLSearchParams(location.search).get('probe-origin') ??
    'https://guest-network.invalid';
  const attempts = `<script>window.__networkAttempted=true;window.__cspViolations=[];addEventListener('securitypolicyviolation',event=>window.__cspViolations.push(event.violatedDirective));fetch(${JSON.stringify(probeOrigin + '/fetch')}).catch(()=>{});const image=new Image();image.src=${JSON.stringify(probeOrigin + '/script-image')};<\/script>`;
  const image = `<img src="${probeOrigin}/raw-image" alt="blocked remote image">`;
  const guestHtml =
    documentMode === 'pre-head-script'
      ? html.replace(
          '<html><body>',
          `<html>${attempts}<head><title>Guest full document</title></head><body>`,
        )
      : documentMode === 'pre-head-image'
        ? html.replace(
            '<html><body>',
            `<html>${image}<head><title>Guest full document</title></head><body>${attempts}`,
          )
        : documentMode === 'comment-fake-head'
          ? html.replace(
              '<html><body>',
              `<!-- <head> is not a real head --> <html>${attempts}${image}<head><title>Guest full document</title></head><body>`,
            )
          : html;
  const snapshot = () => ({
    checked: true,
    components: [structuredClone(authoritative)],
    notices: [],
  });
  function check(token: string, review?: PluginReview) {
    const doc = docs.get(token);
    if (
      !doc ||
      doc.closed ||
      doc.fingerprint !== componentFingerprint([authoritative]) ||
      (review &&
        JSON.stringify(review) !== JSON.stringify(authoritative.review))
    )
      throw { code: 'stale', message: 'https://private.invalid/?token=SECRET' };
  }
  const api = {
    async pageDocument(
      pluginId: string,
      pageId: string,
    ): Promise<PageDocument> {
      documents++;
      const token = `document-${documents}`;
      docs.set(token, {
        fingerprint: componentFingerprint([authoritative]),
        closed: false,
      });
      return {
        plugin_id: pluginId,
        page_id: pageId,
        html: guestHtml,
        document_review: token,
        components: [structuredClone(authoritative)],
      };
    },
    async pageClose(token: string) {
      const doc = docs.get(token);
      if (doc && !doc.closed) {
        doc.closed = true;
        closes++;
      }
    },
    async pageCheck(token: string) {
      check(token);
    },
    async pageConfigure(token: string, review: PluginReview) {
      check(token, review);
      return snapshot();
    },
    async pageInvoke(token: string, review: PluginReview, action: string) {
      check(token, review);
      invokes++;
      if (action === 'late')
        return new Promise((resolve) => (pendingInvoke = resolve));
      if (action === 'huge') return 'x'.repeat(1024 * 1024);
      return { state: 'current', document: token };
    },
    async pageSdk(
      token: string,
      review: PluginReview,
      call: PluginSdkCall,
    ): Promise<PluginSdkReply> {
      check(token, review);
      sdkCalls++;
      if (call.method === 'browser_open')
        return { version: 1, ok: true, value: { state: 'accepted' } };
      return {
        version: 1,
        ok: true,
        value: {
          state: 'file-read',
          selectedPath: (call.arguments as { selectedPath: string })
            .selectedPath,
        },
      };
    },
    async pageProtectedLoad() {
      throw new Error('fixture does not support protected load');
    },
    async pagePreflight(
      token: string,
      review: PluginReview,
      call: PluginSdkCall,
    ) {
      check(token, review);
      preflights++;
      if (
        (call.arguments as Record<string, unknown>).selectedPath !== undefined
      )
        throw new Error('guest path passed preflight');
      return {
        version: 1 as const,
        ok: true,
        value: { state: 'permission-checked' },
      };
    },
    async snapshot() {
      snapshots++;
      return snapshot();
    },
  } as unknown as Pick<
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
  function publish() {
    components = [structuredClone(authoritative)];
  }
  function mutate(
    kind: 'version' | 'revision' | 'registration' | 'disable' | 'revoke',
    notify = true,
  ) {
    if (kind === 'version') {
      authoritative.version = '2.0.0';
      authoritative.review!.identity = 'signed-fixture-v2';
    } else if (kind === 'disable') authoritative.enabled = false;
    else if (kind === 'revoke') authoritative.permissions[0].granted = false;
    else authoritative.review![kind]++;
    if (notify) publish();
  }
  function hidden(value: boolean) {
    Object.defineProperty(document, 'hidden', {
      configurable: true,
      get: () => value,
    });
    document.dispatchEvent(new Event('visibilitychange'));
  }
  async function chooseRead() {
    pickers++;
    return new Promise<string | null>((resolve) => (pendingPicker = resolve));
  }
</script>

<main class="flex h-dvh min-h-0 flex-col gap-2 p-3">
  <div class="flex flex-wrap gap-2">
    <Button onclick={() => mutate('version')}>New installation</Button><Button
      onclick={() => mutate('registration')}>New registration</Button
    ><Button onclick={() => mutate('revision')}>New revision</Button><Button
      onclick={() => mutate('disable')}>Disable component</Button
    ><Button onclick={() => mutate('revoke', false)}>Revoke silently</Button>
    <Button onclick={() => pendingInvoke?.({ state: 'late-canary' })}
      >Resolve old operation</Button
    ><Button onclick={() => pendingPicker?.('/host/chosen')}
      >Finish picker</Button
    ><Button onclick={() => pendingPicker?.(null)}>Cancel picker</Button><Button
      onclick={() => hidden(true)}>Background</Button
    ><Button onclick={() => hidden(false)}>Foreground</Button>
  </div>
  <output aria-label="Host counters"
    >{JSON.stringify({
      documents,
      closes,
      invokes,
      sdkCalls,
      preflights,
      pickers,
      snapshots,
    })}</output
  >
  <PluginPageFrame
    pluginId="fixture.plugin"
    pageId="manage"
    title="Signed fixture"
    {components}
    initialRoute={route}
    {api}
    {chooseRead}
    onSnapshot={(value) => {
      components = value.components;
    }}
  />
</main>
