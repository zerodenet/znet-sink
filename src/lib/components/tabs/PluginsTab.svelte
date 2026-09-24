<script lang="ts">
  import PluginCatalog from './PluginCatalog.svelte';
  import PluginPageFrame from './PluginPageFrame.svelte';
  import DraggableModal from '$lib/components/DraggableModal.svelte';
  import './plugins.css';
  import { ArrowLeft, ArrowUpCircle, ExternalLink, LayoutGrid, List, Play, Plus, Puzzle, RefreshCw, Search, ShieldCheck, Square, Trash2 } from '@lucide/svelte';
  import * as SegmentedControl from '$lib/components/AppSegmentedControl';
  import { Input } from '$lib/components/ui/input';
  import FieldSelect from '$lib/components/ui/select/field-select.svelte';
  import { Switch } from '$lib/components/ui/switch';
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { openExternalUrl as openUrl } from '$lib/services/platform';
  import { Choice } from '$lib/components/ui/choice';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { getAppErrorInfo } from '$lib/services/core';
  import { pluginApi, type PluginComponent, type PluginInstallReview, type PluginPage, type PluginSnapshot } from '$lib/services/plugins';
  import { canApprove, initialPermissionSelection, requiredPermissionSelection, selectedGrants, supportedPermissionSelection, permissionKey, permissionLabel } from '$lib/services/plugin-permissions';
  import * as toast from '$lib/services/toast.svelte';
  import { pluginNavigation, type PluginNavigationRequest } from '$lib/services/plugin-navigation.svelte';
  import { availablePluginUpdate, pluginChannelLabel, type PluginChannel } from '$lib/services/plugin-update-policy';
  import type { PluginListing, PluginRelease } from '$lib/services/plugins';

  type InstalledPlugin = {
    id: string;
    name: string;
    description: string;
    version: string;
    publisher: string;
    repository?: string;
    homepage?: string | null;
    documentation?: string | null;
    license?: string;
    surfaces: string[];
    pages: PluginPage[];
    components: PluginComponent[];
  };

  const INSTALLED_VIEW_MODE_KEY = 'znet-installed-plugins-view-mode';
  const RELEASE_CHANNEL_KEY = 'znet-plugin-release-channel';
  type ViewMode = 'card' | 'list';

  let installedQuery = $state('');
  let installedViewMode = $state<ViewMode>('list');
  let releaseChannel = $state<PluginChannel>('stable');
  let updateCandidates = $state<Record<string, PluginRelease>>({});
  let updateCheckMessage = $state('');
  let updateCheckError = $state('');
  let checkingUpdates = $state(false);
  let requestedUpdate = $state<{ pluginId: string; tag: string } | null>(null);
  let pendingAuthorization = $state<string | null>(null);
  let selectedPluginId = $state<string | null>(null);
  let initialRoute = $state<PluginNavigationRequest | null>(null);
  let detailSection = $state<'manage' | 'permissions' | 'about'>('manage');
  let supported = $state<boolean | null>(null);
  let snapshot = $state<PluginSnapshot>({ checked: false, components: [], notices: [] });
  let busy = $state(false);
  let runningKey = $state<string | null>(null);
  let message = $state('');
  let error = $state('');
  let result = $state('');
  let resultTitle = $state('');
  let resultDescription = $state('');
  let permissionSelections = $state<Record<string, string[]>>({});
  let configurationDrafts = $state<Record<string, Record<string, string>>>({});
  let configurationErrors = $state<Record<string, string>>({});
  let removeOpen = $state(false);
  let removePlugin = $state<InstalledPlugin | null>(null);
  let installOpen = $state(false);
  let localInstallPath = $state('');
  let localInstallReview = $state<PluginInstallReview | null>(null);
  let localInstallError = $state('');
  let runGeneration = 0;
  let updateGeneration = 0;
  let alive = true;

  const allPlugins = $derived.by(() => {
    const groups = new Map<string, InstalledPlugin>();
    for (const component of snapshot.components) {
      const plugin = groups.get(component.plugin_id);
      if (plugin) {
        plugin.components.push(component);
        for (const surface of component.surfaces ?? []) if (!plugin.surfaces.includes(surface)) plugin.surfaces.push(surface);
        continue;
      }
      groups.set(component.plugin_id, {
        id: component.plugin_id,
        name: component.name,
        description: component.description?.trim() || '此插件尚未提供说明。',
        version: component.version,
        publisher: component.publisher,
        repository: component.repository,
        homepage: component.homepage,
        documentation: component.documentation,
        license: component.license,
        surfaces: [...(component.surfaces ?? [])],
        pages: (snapshot.pages ?? []).filter(page => page.plugin_id === component.plugin_id),
        components: [component],
      });
    }
    return [...groups.values()];
  });
  const installedPlugins = $derived(allPlugins.filter(plugin =>
    `${plugin.name} ${plugin.id} ${plugin.publisher} ${plugin.description}`.toLowerCase().includes(installedQuery.trim().toLowerCase())));
  const selectedPlugin = $derived(allPlugins.find(plugin => plugin.id === selectedPluginId) ?? null);
  const selectedManagementPage = $derived(selectedPlugin?.pages.find(page => page.kind === 'management') ?? null);
  let reloadGeneration = 0;

  async function reload(force = false) {
    const generation = ++reloadGeneration;
    const next = await pluginApi.snapshot();
    if (alive && generation === reloadGeneration && (force || selectedPluginId === null)) snapshot = next;
  }
  onMount(() => {
    alive = true;
    try { installedViewMode = localStorage.getItem(INSTALLED_VIEW_MODE_KEY) === 'card' ? 'card' : 'list'; }
    catch { installedViewMode = 'list'; }
    try {
      const savedChannel = localStorage.getItem(RELEASE_CHANNEL_KEY);
      releaseChannel = savedChannel === 'rc' || savedChannel === 'dev' ? savedChannel : 'stable';
    } catch { releaseChannel = 'stable'; }
    void pluginApi.supported().then(async value => { if (!alive) return; supported = value; if (value) { await reload(); void checkUpdates(); } })
      .catch(reason => { error = getAppErrorInfo(reason, '无法读取插件状态').message; });
    const timer = setInterval(() => {
      if (supported && selectedPluginId === null) void reload().catch(() => {});
    }, 3000);
    return () => { alive = false; runGeneration++; reloadGeneration++; updateGeneration++; clearInterval(timer); };
  });
  function clearResult() {
    result = ''; resultTitle = ''; resultDescription = '';
  }

  function setInstalledViewMode(mode: ViewMode) {
    installedViewMode = mode;
    try { localStorage.setItem(INSTALLED_VIEW_MODE_KEY, mode); }
    catch { /* View preference persistence is best effort. */ }
  }
  function openInstallModal() {
    requestedUpdate = null;
    localInstallPath = '';
    localInstallReview = null;
    localInstallError = '';
    installOpen = true;
  }
  async function checkUpdates(catalog?: PluginListing[], components = snapshot.components) {
    const generation = ++updateGeneration;
    checkingUpdates = true;
    updateCheckMessage = '';
    updateCheckError = '';
    updateCandidates = {};
    try {
      const listings = catalog ?? await pluginApi.catalog();
      const registered = new Set(listings.map(listing => listing.id));
      const versions = new Map(components.map(component => [component.plugin_id, component.version]));
      const ids = [...versions.keys()].filter(id => registered.has(id));
      const checked = await Promise.allSettled(ids.map(id => pluginApi.releases(id)));
      const found: Record<string, PluginRelease> = {};
      let unavailable = 0;
      for (const [index, outcome] of checked.entries()) {
        if (outcome.status === 'rejected') { unavailable++; continue; }
        const candidate = availablePluginUpdate(versions.get(ids[index])!, outcome.value, releaseChannel);
        if (candidate) found[ids[index]] = candidate;
      }
      if (!alive || generation !== updateGeneration) return;
      updateCandidates = found;
      const unpublished = versions.size - ids.length;
      const result = Object.keys(found).length ? `${Object.keys(found).length} 个有更新` : '没有发现适用更新';
      updateCheckMessage = unavailable
        ? `已按${pluginChannelLabel[releaseChannel]}检查 ${ids.length - unavailable} 个插件：${result}；${unavailable} 个发行信息暂不可用，无法判断其更新状态${unpublished ? `；${unpublished} 个未在市场登记` : ''}。`
        : `已按${pluginChannelLabel[releaseChannel]}检查 ${versions.size} 个已安装插件：${result}${unpublished ? `；${unpublished} 个未在市场登记` : ''}。`;
    } catch (reason) {
      if (alive && generation === updateGeneration) updateCheckError = `市场不可用，暂时无法判断插件更新：${getAppErrorInfo(reason, '检查失败').message}`;
    } finally { if (alive && generation === updateGeneration) checkingUpdates = false; }
  }
  async function refreshInstalled() {
    busy = true; error = ''; message = '';
    try {
      const next = await pluginApi.refresh();
      snapshot = next;
      await checkUpdates(undefined, next.components);
    } catch (reason) {
      updateCandidates = {};
      updateCheckMessage = '';
      updateCheckError = `检查已安装插件失败：${getAppErrorInfo(reason, '检查失败').message}`;
    } finally { busy = false; }
  }
  function selectReleaseChannel(value: string) {
    if (value !== 'stable' && value !== 'rc' && value !== 'dev') return;
    releaseChannel = value;
    try { localStorage.setItem(RELEASE_CHANNEL_KEY, value); } catch { /* Preference is best effort. */ }
    void checkUpdates();
  }
  function openUpdate(pluginId: string, tag: string) {
    requestedUpdate = { pluginId, tag };
    localInstallReview = null;
    installOpen = true;
  }
  function installationFinished(next: PluginSnapshot, review: PluginInstallReview) {
    snapshot = next;
    installOpen = false;
    requestedUpdate = null;
    localInstallError = '';
    const requiresAuthorization = review.first_install || !review.current_version || review.added_permissions.length > 0
      || next.components.some(component => component.plugin_id === review.plugin_id && !component.enabled
        && component.permissions.some(permission => permission.required && !permission.granted));
    pendingAuthorization = requiresAuthorization ? review.plugin_id : null;
    message = requiresAuthorization
      ? `${review.first_install || !review.current_version ? '插件包已安装' : '安装包已更新'}，运行权限尚未确认。请检查权限后启用插件。`
      : next.components.some(component => component.plugin_id === review.plugin_id && component.enabled)
        ? '插件已更新，原有启用状态和权限已保留。'
        : '插件已更新，仍保持原来的停用状态。';
    error = '';
    void checkUpdates(undefined, next.components);
  }
  async function action(work: () => Promise<PluginSnapshot>, success: string): Promise<boolean> {
    busy = true; error = ''; message = ''; clearResult();
    try { snapshot = await work(); message = success; return true; }
    catch (reason) { error = getAppErrorInfo(reason, '插件操作失败').message; await reload(true).catch(() => {}); return false; }
    finally { busy = false; }
  }
  async function installLocal() {
    try {
      const path = await open({ title: '选择插件包', multiple: false, directory: false, filters: [{ name: 'ZNet Sink 插件', extensions: ['zspkg'] }] });
      if (typeof path !== 'string') return;
      busy = true; error = ''; message = ''; localInstallError = '';
      const review = await pluginApi.previewInstall(path);
      if (review.requires_approval) {
        localInstallPath = path;
        localInstallReview = review;
        return;
      }
      snapshot = await pluginApi.install(path);
      installOpen = false;
      message = '插件已安装';
      toast.success('插件已安装');
    } catch (reason) {
      localInstallError = getAppErrorInfo(reason, '本地插件预检失败').message;
      toast.error(localInstallError);
    }
    finally { busy = false; }
  }
  function cancelLocalInstallReview() {
    if (busy) return;
    localInstallPath = '';
    localInstallReview = null;
    localInstallError = '';
  }
  async function confirmLocalUpgrade() {
    if (!localInstallReview || !localInstallPath) return;
    busy = true; error = ''; message = ''; localInstallError = '';
    try {
      const next = await pluginApi.install(localInstallPath, localInstallReview.candidate_digest);
      installationFinished(next, localInstallReview);
      localInstallPath = '';
      localInstallReview = null;
      toast.success(pendingAuthorization ? '插件已安装，待确认运行权限' : '插件已更新');
    } catch (reason) {
      localInstallError = getAppErrorInfo(reason, '本地插件安装失败').message;
      toast.error(localInstallError);
    }
    finally { busy = false; }
  }
  function componentKey(component: PluginComponent) {
    return component.review?.key ?? `${component.plugin_id}/${component.component_id}`;
  }
  function openDetails(plugin: InstalledPlugin, tab: typeof detailSection = 'manage', route: PluginNavigationRequest | null = null) {
    reloadGeneration++;
    selectedPluginId = plugin.id;
    initialRoute = route;
    detailSection = tab;
    permissionSelections = Object.fromEntries(plugin.components.map(component => [componentKey(component), initialPermissionSelection(component)]));
    configurationDrafts = Object.fromEntries(plugin.components.filter(component => component.configuration).map(component => [componentKey(component), { ...component.configuration!.values }]));
    configurationErrors = {};
    message = ''; error = ''; clearResult();
  }
  function closeDetails() {
    selectedPluginId = null;
    initialRoute = null;
    message = ''; error = ''; clearResult();
    void reload().catch(() => {});
  }

  $effect(() => {
    const request = pluginNavigation.pending;
    if (!request || !snapshot.checked) return;
    const plugin = allPlugins.find(value => value.id === request.pluginId);
    if (!plugin || !plugin.pages.some(page => page.id === request.pageId)) return;
    openDetails(plugin, 'manage', request);
    pluginNavigation.clear(request.token);
  });
  function pluginEnabled(plugin: InstalledPlugin) {
    return plugin.components.some(component => component.enabled || component.running || runningKey === component.review?.key);
  }
  function pluginStatus(plugin: InstalledPlugin) {
    if (plugin.components.every(component => !!component.blocked)) return '暂不可用';
    if (plugin.components.some(component => component.configuration?.configured === false)) return '待配置';
    if (plugin.components.some(component => component.permission_review_required)) return '待确认权限';
    const count = plugin.components.filter(component => component.enabled || component.running).length;
    if (!count) return '未启用';
    return count === plugin.components.length ? '已启用' : '部分启用';
  }
  async function togglePlugin(plugin: InstalledPlugin, checked: boolean) {
    if (checked) {
      openDetails(plugin, plugin.components.some(component => component.configuration?.configured === false) ? 'manage' : 'permissions');
      message = '请在详情页检查配置和权限后启用插件';
      return;
    }
    busy = true; error = ''; message = ''; clearResult(); ++runGeneration;
    try {
      let next = snapshot;
      for (const component of plugin.components) {
        if (component.review && (component.enabled || component.running)) next = await pluginApi.stop(component.review.key);
      }
      snapshot = next;
      message = '插件已停用；已批准权限会在下次启用时继续使用';
    } catch (reason) { error = getAppErrorInfo(reason, '停用失败').message; await reload(true).catch(() => {}); }
    finally { busy = false; }
  }
  function changePermission(component: PluginComponent, key: string, checked: boolean) {
    const id = componentKey(component);
    const current = permissionSelections[id] ?? [];
    permissionSelections = { ...permissionSelections, [id]: checked ? [...current.filter(value => value !== key), key] : current.filter(value => value !== key) };
  }
  function setPermissionSelection(component: PluginComponent, selection: 'required' | 'all') {
    const id = componentKey(component);
    permissionSelections = {
      ...permissionSelections,
      [id]: selection === 'all' ? supportedPermissionSelection(component) : requiredPermissionSelection(component),
    };
  }
  function openPermissionGuide(pluginId: string) {
    const components = snapshot.components.filter(component => component.plugin_id === pluginId);
    reloadGeneration++;
    selectedPluginId = pluginId;
    initialRoute = null;
    detailSection = 'permissions';
    pendingAuthorization = null;
    permissionSelections = Object.fromEntries(components.map(component => [componentKey(component), initialPermissionSelection(component)]));
    configurationDrafts = Object.fromEntries(components.filter(component => component.configuration).map(component => [componentKey(component), { ...component.configuration!.values }]));
    configurationErrors = {};
    clearResult();
  }
  async function approve(component: PluginComponent) {
    if (!component.review) return;
    const selected = permissionSelections[componentKey(component)] ?? [];
    if (!canApprove(component, selected)) return;
    const succeeded = await action(() => pluginApi.authorize(component.review!, selectedGrants(component, selected)), '组件已启用');
    if (succeeded && !snapshot.components.some(value => value.plugin_id === component.plugin_id && value.permission_review_required)) pendingAuthorization = null;
  }
  async function run(component: PluginComponent) {
    if (!component.review) return;
    const generation = ++runGeneration;
    runningKey = component.review.key; error = ''; message = ''; clearResult();
    try {
      const value = await pluginApi.run(component.review);
      if (generation === runGeneration && alive) {
        result = JSON.stringify(value, null, 2) ?? 'null';
        resultTitle = isFoundation(component) ? '基础组件自检完成' : '插件操作完成';
        resultDescription = isFoundation(component)
          ? '安装包、配置传递和沙箱启动均已完成。这个基础组件不会执行设备授权、订阅同步或消息读取。'
          : '插件已完成操作；技术结果可按需展开查看。';
        message = resultTitle;
      }
    } catch (reason) {
      if (generation === runGeneration && alive) error = getAppErrorInfo(reason, '运行失败').message;
    } finally { if (alive) { runningKey = null; await reload(true).catch(() => {}); } }
  }
  async function stop(component: PluginComponent) {
    if (!component.review) return;
    ++runGeneration;
    try { snapshot = await pluginApi.stop(component.review.key); clearResult(); message = '组件已停用；已批准权限仍然保留'; error = ''; }
    catch (reason) { error = getAppErrorInfo(reason, '停用失败').message; }
  }
  async function revokePermissions(component: PluginComponent) {
    if (!component.review) return;
    ++runGeneration;
    await action(() => pluginApi.revokePermissions(component.review!.key), '组件权限已撤销');
    const key = componentKey(component);
    permissionSelections = { ...permissionSelections, [key]: [] };
  }
  function isFoundation(component: PluginComponent) {
    return component.component_id === 'foundation' && component.permissions.length === 0;
  }
  function componentLabel(component: PluginComponent) {
    return isFoundation(component) ? '基础验证组件' : component.component_id;
  }
  function configurationValid(component: PluginComponent, values: Record<string, string>) {
    if (!component.configuration) return false;
    return component.configuration.schema.fields.every(field => {
      const value = values[field.id] ?? field.default ?? '';
      if (field.required && !value) return false;
      if (field.kind === 'select') return !value || field.options.some(option => option.value === value);
      if (field.kind === 'https_origin' && value) {
        try {
          const url = new URL(value);
          return url.protocol === 'https:' && !url.username && !url.password && url.pathname === '/' && !url.search && !url.hash;
        } catch { return false; }
      }
      if (field.kind === 'https_origin_list' && value) {
        try {
          const origins = JSON.parse(value);
          return Array.isArray(origins) && origins.length <= 16
            && new Set(origins).size === origins.length
            && origins.every(origin => {
              if (typeof origin !== 'string') return false;
              const url = new URL(origin);
              return url.protocol === 'https:' && !url.username && !url.password && url.pathname === '/' && !url.search && !url.hash;
            });
        } catch { return false; }
      }
      return value.length <= 2048;
    });
  }
  async function saveConfiguration(component: PluginComponent) {
    if (!component.review) return;
    const key = componentKey(component);
    const values = configurationDrafts[key] ?? {};
    if (!configurationValid(component, values)) return;
    busy = true; configurationErrors = { ...configurationErrors, [key]: '' }; message = ''; error = ''; clearResult();
    try {
      snapshot = await pluginApi.configure(component.review.key, $state.snapshot(values));
      message = component.enabled ? '配置已保存，组件已按新配置恢复' : '配置已保存';
    } catch (reason) {
      configurationErrors = { ...configurationErrors, [key]: getAppErrorInfo(reason, '保存插件配置失败').message };
    } finally { busy = false; }
  }
  async function openExternal(url?: string | null) {
    if (!url) return;
    try { await openUrl(url); }
    catch (reason) { error = getAppErrorInfo(reason, '无法打开链接').message; }
  }
</script>

<div class="desk-card plugins-panel animate-fade-in">
  {#if selectedPlugin}
    <div class="plugins-detail-header"><Button variant="ghost" size="sm" onclick={closeDetails}><ArrowLeft size={15} />返回已安装插件</Button><span title={selectedPlugin.name}>{selectedPlugin.name}</span></div>
  {:else}
    <div class="plugins-header">
      <div class="plugins-title-group"><h2>插件管理</h2><p>查看已安装插件，管理状态、权限和设置</p></div>
      {#if supported}<Button size="sm" disabled={busy || runningKey !== null} onclick={openInstallModal}><Plus size={14} />安装插件</Button>{/if}
    </div>
  {/if}

  {#if supported === null}<div class="plugins-empty" role="status">正在读取插件状态…</div>
  {:else if !supported}<div class="plugins-empty"><Puzzle size={28} />当前设备尚未开放插件运行。</div>
  {:else if selectedPlugin}
    <div class="plugins-detail-scroll" class:plugin-management-active={detailSection === 'manage' && !!selectedManagementPage}>
      {#if updateCandidates[selectedPlugin.id]}
        <div class="plugin-update-banner" role="status"><ArrowUpCircle size={17} /><span>可更新至 {updateCandidates[selectedPlugin.id].tag_name}（{pluginChannelLabel[releaseChannel]}）</span><Button size="sm" onclick={() => openUpdate(selectedPlugin.id, updateCandidates[selectedPlugin.id].tag_name)}>查看更新</Button></div>
      {/if}
      {#if selectedPlugin.components.some(component => component.permission_review_required)}
        <div class="plugin-authorization-notice" role="status"><span>新版插件已安装，但新增或必需的运行权限尚未确认，因此暂未启用。</span><Button size="sm" onclick={() => { detailSection = 'permissions'; }}>检查权限</Button></div>
      {/if}
      <div class="plugin-detail-toolbar">
        <SegmentedControl.Root value={detailSection} onValueChange={(value) => { detailSection = value as typeof detailSection; }} aria-label="插件详情栏目">
          <SegmentedControl.Item value="manage">管理</SegmentedControl.Item><SegmentedControl.Item value="permissions">权限</SegmentedControl.Item><SegmentedControl.Item value="about">关于</SegmentedControl.Item>
        </SegmentedControl.Root>
        <label class="plugin-master-switch"><strong>{pluginStatus(selectedPlugin)}</strong><Switch size="sm" checked={pluginEnabled(selectedPlugin)} disabled={busy || runningKey !== null || !snapshot.checked} onCheckedChange={(checked) => void togglePlugin(selectedPlugin, checked)} aria-label={pluginEnabled(selectedPlugin) ? '停用插件' : '启用插件'} /></label>
      </div>

      {#if detailSection === 'manage'}
        {#if selectedManagementPage}
          <div class="plugin-management-surface">
            <PluginPageFrame pluginId={selectedPlugin.id} pageId={selectedManagementPage.id} title={selectedManagementPage.title} components={selectedPlugin.components} {initialRoute} onSnapshot={(value) => { snapshot = value; }} />
          </div>
        {:else}
          <section class="plugin-detail-section"><div class="plugin-section-heading"><div><h3>基础配置与诊断</h3><p>此版本没有提供独立管理页面。这里仅保留宿主声明式配置和开发诊断。</p></div></div></section>
          {#each selectedPlugin.components as component (componentKey(component))}
            <section class="plugin-detail-section">
              <div class="plugin-section-heading"><div><h3>{componentLabel(component)}</h3><p>{isFoundation(component) ? '用于验证安装、配置传递和沙箱启动，不包含业务接入。' : '由插件提供的独立服务组件。'}</p></div><span class="plugin-status" class:authorized={component.enabled && !component.blocked} class:blocked={!!component.blocked}>{component.blocked ? '暂不可用' : component.configuration?.configured === false ? '待配置' : component.enabled ? '已启用' : '未启用'}</span></div>
              {#if component.blocked}<p class="plugins-error">{component.blocked}</p>{/if}
              {#if component.configuration}
                <div class="plugin-configuration-grid">
                  <div class="plugin-configuration-intro"><strong>{component.configuration.schema.title}</strong>{#if component.configuration.schema.description}<p>{component.configuration.schema.description}</p>{/if}</div>
                  {#each component.configuration.schema.fields as field (field.id)}
                    <label class="configuration-field">
                      <span>{field.label}{field.required ? ' *' : ''}</span>
                      {#if field.kind === 'select'}
                        <FieldSelect aria-label={field.label} value={configurationDrafts[componentKey(component)]?.[field.id] ?? field.default ?? ''} onValueChange={(value) => { configurationDrafts = { ...configurationDrafts, [componentKey(component)]: { ...(configurationDrafts[componentKey(component)] ?? {}), [field.id]: value } }; }} options={field.options} />
                      {:else}
                        <Input aria-label={field.label} type={field.kind === 'https_origin' ? 'url' : 'text'} value={configurationDrafts[componentKey(component)]?.[field.id] ?? field.default ?? ''} oninput={(event) => { configurationDrafts = { ...configurationDrafts, [componentKey(component)]: { ...(configurationDrafts[componentKey(component)] ?? {}), [field.id]: event.currentTarget.value } }; }} placeholder={field.kind === 'https_origin' ? 'https://panel.example.com' : field.kind === 'https_origin_list' ? '["https://panel.example.com"]' : ''} />
                      {/if}
                      {#if field.description}<small>{field.description}</small>{/if}
                    </label>
                  {/each}
                  {#if configurationErrors[componentKey(component)]}<p class="plugins-error" role="alert">{configurationErrors[componentKey(component)]}</p>{/if}
                  <div class="plugin-section-actions"><Button size="sm" disabled={busy || !configurationValid(component, configurationDrafts[componentKey(component)] ?? {})} onclick={() => saveConfiguration(component)}>保存配置</Button></div>
                </div>
              {/if}
              <details class="plugin-diagnostics"><summary>开发诊断</summary><div class="plugin-section-actions">{#if component.enabled}<Button size="sm" variant="outline" onclick={() => stop(component)}><Square size={14} />停用组件</Button>{/if}<Button size="sm" disabled={busy || runningKey !== null || !component.enabled || !!component.blocked} onclick={() => run(component)}><Play size={14} />运行诊断</Button></div>{#if result}<div class="plugins-result" role="status"><strong>{resultTitle}</strong><p>{resultDescription}</p><details><summary>查看技术结果</summary><pre>{result}</pre></details></div>{/if}</details>
            </section>
          {/each}
        {/if}
      {:else if detailSection === 'permissions'}
        <section class="plugin-detail-section">
          <div class="plugin-section-heading"><div><h3>访问权限</h3><p>权限名称、访问范围和授权状态由客户端固定展示，插件不能修改或隐藏。</p></div></div>
          {#if selectedPlugin.components.some(component => component.permission_review_required || !component.enabled && component.permissions.some(permission => permission.required))}
            <p class="plugins-notice">启用组件前需要确认权限。必需权限已预选；你可以直接启用，也可以一键选择全部可用权限。</p>
          {/if}
          {#each selectedPlugin.components as component (componentKey(component))}
            <div class="plugin-permission-group">
              <div class="plugin-section-heading"><div><strong>{componentLabel(component)}</strong><p>批准结果由客户端持久化；新增权限或扩大范围时必须重新确认。</p></div><span class="plugin-status" class:authorized={component.enabled}>{component.enabled ? '已启用' : component.permissions.some(permission => permission.granted) ? '已停用' : '未授权'}</span></div>
              {#if component.permissions.some(permission => permission.supported)}
                <div class="plugin-permission-tools">
                  <span>已选择 {(permissionSelections[componentKey(component)] ?? []).length} / {component.permissions.filter(permission => permission.supported).length} 项</span>
                  <div>
                    <Button size="sm" variant="ghost" disabled={busy || component.enabled} onclick={() => setPermissionSelection(component, 'required')}>仅选必需</Button>
                    <Button size="sm" variant="outline" disabled={busy || component.enabled} onclick={() => setPermissionSelection(component, 'all')}>全选可用权限</Button>
                  </div>
                </div>
              {/if}
              {#each component.permissions as permission}
                <label class="permission-row"><Choice checked={(permissionSelections[componentKey(component)] ?? []).includes(permissionKey(permission.request))} onchange={(event) => changePermission(component, permissionKey(permission.request), event.currentTarget.checked)} disabled={!permission.supported || component.enabled} /><span>{permissionLabel(permission.request.capability)}{permission.required ? '（必需）' : '（可选）'}<small>{permission.request.scope === 'self' ? '仅此插件的身份信息' : permission.request.scope}{permission.supported ? '' : ' · 暂不可用'}</small></span></label>
              {/each}
              {#if component.permissions.length === 0}<p class="plugin-muted-block">此组件没有申请宿主访问权限。</p>{/if}
              <div class="plugin-section-actions">{#if component.permissions.some(permission => permission.granted)}<Button size="sm" variant="ghost" disabled={busy} onclick={() => revokePermissions(component)}>撤销已批准权限</Button>{/if}{#if component.enabled}<Button size="sm" variant="outline" onclick={() => stop(component)}><Square size={14} />停用组件</Button>{:else}<Button size="sm" disabled={busy || !canApprove(component, permissionSelections[componentKey(component)] ?? [])} onclick={() => approve(component)}><ShieldCheck size={14} />允许并启用</Button>{/if}</div>
            </div>
          {/each}
        </section>
      {:else}
        <section class="plugin-detail-section plugin-about">
          <div class="plugin-about-summary">
            <div class="plugin-icon"><Puzzle size={18} /></div>
            <div class="plugin-about-identity">
              <div class="plugin-title-row"><h3>{selectedPlugin.name}</h3><span class="plugin-detail-version" title={selectedPlugin.version}>{selectedPlugin.version}</span></div>
              <p>{selectedPlugin.description}</p>
            </div>
          </div>
          <dl><div><dt>插件 ID</dt><dd title={selectedPlugin.id}>{selectedPlugin.id}</dd></div><div><dt>发布者</dt><dd>{selectedPlugin.publisher}</dd></div>{#if selectedPlugin.license}<div><dt>许可证</dt><dd>{selectedPlugin.license}</dd></div>{/if}<div><dt>组件</dt><dd>{selectedPlugin.components.map(componentLabel).join('、')}</dd></div></dl>
          <div class="plugin-about-links">{#if selectedPlugin.repository}<Button variant="outline" size="sm" onclick={() => openExternal(selectedPlugin.repository)}><ExternalLink size={14} />代码仓库</Button>{/if}{#if selectedPlugin.documentation}<Button variant="outline" size="sm" onclick={() => openExternal(selectedPlugin.documentation)}><ExternalLink size={14} />使用文档</Button>{/if}{#if selectedPlugin.homepage}<Button variant="outline" size="sm" onclick={() => openExternal(selectedPlugin.homepage)}><ExternalLink size={14} />主页</Button>{/if}</div>
          <div class="plugin-danger-zone"><div><strong>卸载插件</strong><p>移除全部组件、配置、插件状态、缓存和已批准权限。</p></div><Button variant="destructive" size="sm" disabled={busy || runningKey !== null} onclick={() => { removePlugin = selectedPlugin; removeOpen = true; }}><Trash2 size={14} />卸载</Button></div>
        </section>
      {/if}
    </div>
  {:else}
    <div class="plugins-toolbar">
      <div class="plugins-toolbar-title"><strong>已安装插件</strong><span>{allPlugins.length} 个</span></div>
      <div class="plugins-toolbar-actions">
        {#if allPlugins.length}
          <SegmentedControl.Root value={installedViewMode} onValueChange={(value) => setInstalledViewMode(value as ViewMode)} aria-label="已安装插件显示方式">
            <SegmentedControl.Item value="card" size="icon" title="卡片视图" aria-label="卡片视图"><LayoutGrid class="h-3.5 w-3.5" /></SegmentedControl.Item>
            <SegmentedControl.Item value="list" size="icon" title="列表视图" aria-label="列表视图"><List class="h-3.5 w-3.5" /></SegmentedControl.Item>
          </SegmentedControl.Root>
        {/if}
        <div class="plugins-search"><Search size={14} aria-hidden="true" /><Input class="pl-8" aria-label="搜索已安装插件" placeholder="搜索已安装插件…" bind:value={installedQuery} /></div>
        <FieldSelect aria-label="插件发行通道" value={releaseChannel} onValueChange={selectReleaseChannel} options={[
          { value: 'stable', label: '正式版' }, { value: 'rc', label: '候选版' }, { value: 'dev', label: '开发版' },
        ]} />
        <Button variant="outline" size="sm" disabled={busy || checkingUpdates || runningKey !== null} onclick={refreshInstalled}><RefreshCw size={14} class={busy || checkingUpdates ? 'animate-spin' : ''} />检查已安装插件</Button>
      </div>
    </div>
    <div class="plugins-scroll">
      <p class="plugins-hint">每个条目代表一个完整插件。点击条目进入详情；状态、权限和卸载始终由客户端管理。</p>
      {#if checkingUpdates}<p class="plugins-notice" role="status">正在核对{pluginChannelLabel[releaseChannel]}发行包…</p>{/if}
      {#if updateCheckMessage}<p class="plugins-notice" role="status">{updateCheckMessage}</p>{/if}
      {#if updateCheckError}<p class="plugins-notice plugins-error" role="alert">{updateCheckError}</p>{/if}
      {#if pendingAuthorization}<div class="plugin-authorization-notice" role="status"><span>插件包已安装，运行权限尚未确认，因此暂未启用。</span><Button size="sm" onclick={() => openPermissionGuide(pendingAuthorization!)}>检查权限并启用</Button></div>{/if}
      {#if !snapshot.checked}<p class="plugins-notice">请先检查插件登记，再确认访问权限。</p>{/if}
      {#each snapshot.notices as notice}<p class="plugins-notice">{notice}</p>{/each}
      {#if snapshot.checked && allPlugins.length === 0}
        <div class="plugins-empty"><Puzzle size={28} /><strong>还没有已安装的插件</strong><p>打开插件市场在线安装，也可以选择本地插件包。</p><Button size="sm" onclick={openInstallModal}><Plus size={14} />安装插件</Button></div>
      {:else if allPlugins.length > 0 && !installedPlugins.length}
        <div class="plugins-empty"><Search size={28} /><p>没有找到匹配的插件。</p><Button variant="ghost" size="sm" onclick={() => { installedQuery = ''; }}>清除搜索</Button></div>
      {:else}
        <div class="plugins-collection installed-plugins" class:card-view={installedViewMode === 'card'} class:list-view={installedViewMode === 'list'}>
          {#each installedPlugins as plugin (plugin.id)}
            <article class="plugin-card installed-plugin-card">
              <button class="plugin-card-open" data-slot="surface-button" type="button" aria-label={`打开 ${plugin.name} 详情`} onclick={() => openDetails(plugin)}></button>
              <div class="plugin-card-heading"><div class="plugin-icon"><Puzzle size={18} /></div><div class="plugin-identity"><strong title={plugin.name}>{plugin.name}</strong><span class="plugin-meta" title={`发布者 ${plugin.publisher}`}>发布者 {plugin.publisher} · 当前 {plugin.version}</span></div>{#if !updateCandidates[plugin.id]}<span class="plugin-version" title={plugin.version}>{plugin.version}</span>{/if}</div>
              {#if updateCandidates[plugin.id]}<Button variant="outline" size="sm" class="plugin-update-indicator" aria-label={`${plugin.name}有更新：${pluginChannelLabel[releaseChannel]} ${updateCandidates[plugin.id].tag_name}`} title={`更新到 ${updateCandidates[plugin.id].tag_name}（${pluginChannelLabel[releaseChannel]}）`} onclick={() => openUpdate(plugin.id, updateCandidates[plugin.id].tag_name)}><ArrowUpCircle size={13} />更新</Button>{/if}
              <p class="plugin-description">{plugin.description}</p><div class="plugin-card-id" title={plugin.id}>ID：{plugin.id}</div>
              <div class="plugin-card-actions plugin-toggle-action"><label class="plugin-card-toggle"><span>{pluginStatus(plugin)}</span><Switch size="sm" checked={pluginEnabled(plugin)} disabled={busy || runningKey !== null || !snapshot.checked} onCheckedChange={(checked) => void togglePlugin(plugin, checked)} aria-label={`${plugin.name}${pluginEnabled(plugin) ? '停用' : '启用'}`} /></label></div>
            </article>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
  {#if message || error}<div class="plugins-feedback" aria-live="polite">{#if message}<p>{message}</p>{/if}{#if error}<p class="plugins-error" role="alert">{error}</p>{/if}</div>{/if}
</div>

<DraggableModal
  title="安装插件"
  description="线上版本信息来自插件中心，安装包直接从作者 GitHub Release 下载；未上架插件也可从本地安装。"
  open={installOpen}
  onClose={() => { if (!busy) { installOpen = false; cancelLocalInstallReview(); } }}
  closeDisabled={busy || runningKey !== null}
  bodyScrollable={false}
  width="min(1080px, calc(100vw - 32px))"
>
  <div class="plugin-install-modal-body">
    <div class="plugin-install-market" class:plugin-install-hidden={!!localInstallReview}>
      <PluginCatalog installed={snapshot.components} channel={releaseChannel} {requestedUpdate} onrefresh={(catalog) => checkUpdates(catalog)} onlocalinstall={installLocal} onmanage={() => { installOpen = false; }} disabled={busy || runningKey !== null} oninstalled={(value, review) => { installationFinished(value, review); toast.success(pendingAuthorization ? '插件已安装，待确认运行权限' : '插件已更新'); }} />
    </div>
    {#if localInstallReview}
      <div class="plugin-local-review">
        <div class="plugin-local-review-nav"><Button variant="ghost" size="sm" disabled={busy} onclick={cancelLocalInstallReview}><ArrowLeft size={14} />返回插件市场</Button><span>{localInstallReview.first_install ? '确认本地插件来源' : '确认插件更新范围'}</span></div>
        <div class="plugin-local-review-scroll">
          <section class="plugin-detail-section">
            <div class="plugin-section-heading"><div><h3>{localInstallReview.first_install ? `安装 ${localInstallReview.plugin_id}` : `更新 ${localInstallReview.plugin_id}`}</h3><p>{localInstallReview.first_install ? `首次安装版本 ${localInstallReview.candidate_version}` : `从 ${localInstallReview.current_version} 更新到 ${localInstallReview.candidate_version}`}。本地安装由你确认发布者和声明，不要求插件先上架市场。</p></div></div>
            <p class="plugins-notice">发布者：{localInstallReview.publisher}<br />签名指纹：<span class="plugin-trust-fingerprint">{localInstallReview.publisher_fingerprint}</span></p>
            <p class="plugins-notice">安装完成后插件保持停用；请在权限页检查并批准实际权限后再启用。同一插件后续更新必须使用相同签名。</p>
            {#if localInstallReview.requested_surfaces.includes('znet-sink.ui.management.v1')}<p class="plugins-notice">此插件提供独立管理页面。</p>{/if}
            <div class="plugin-permission-group">
              {#if localInstallReview.added_permissions.length === 0}<p class="plugin-muted-block">此版本没有新增宿主权限。</p>{/if}
              {#each localInstallReview.added_permissions as change}
                <div class="permission-row"><span><strong>{permissionLabel(change.request.capability)}</strong>{change.required ? '（必需）' : '（可选）'}<small>组件 {change.component_id} · {change.request.scope}</small></span></div>
              {/each}
            </div>
            {#if localInstallError}<p class="plugins-error" role="alert">{localInstallError}</p>{/if}
          </section>
        </div>
        <div class="plugin-local-review-footer"><Button variant="outline" disabled={busy} onclick={cancelLocalInstallReview}>返回</Button><Button disabled={busy} onclick={confirmLocalUpgrade}>{busy ? '正在安装…' : localInstallReview.first_install ? '信任并安装' : '确认并更新'}</Button></div>
      </div>
    {/if}
  </div>
</DraggableModal>

<Dialog.Root bind:open={removeOpen}>
  <Dialog.Content class="sm:max-w-[420px]"><Dialog.Header><Dialog.Title>卸载插件</Dialog.Title><Dialog.Description>卸载 {removePlugin?.name} 的全部组件，并清除配置、插件状态、缓存和已批准权限？</Dialog.Description></Dialog.Header><Dialog.Footer><Button variant="outline" onclick={() => { removeOpen = false; }}>取消</Button><Button variant="destructive" onclick={() => { const id = removePlugin?.id; removeOpen = false; if (id) void action(() => pluginApi.uninstall(id), '插件已卸载').then(success => { if (success) selectedPluginId = null; }); }}>卸载</Button></Dialog.Footer></Dialog.Content>
</Dialog.Root>

<style>
  .plugin-permission-tools { display: flex; align-items: center; justify-content: space-between; gap: 10px; color: var(--muted-foreground); font-size: 11px; }
  .plugin-permission-tools > div { display: flex; align-items: center; gap: 6px; }
  .permission-row { display: flex; align-items: flex-start; gap: 10px; padding: 10px; border: 1px solid var(--border); border-radius: 8px; font-size: 12px; }
  .permission-row :global([data-slot='choice']) { margin-top: 3px; flex-shrink: 0; }
  .permission-row span { min-width: 0; }
  small { display: block; margin-top: 4px; color: var(--muted-foreground); font-size: 11px; overflow-wrap: anywhere; }
  .configuration-field { display: grid; gap: 7px; min-width: 0; font-size: 12px; font-weight: 500; }
  .configuration-field small { margin-top: 0; font-weight: 400; }
</style>
