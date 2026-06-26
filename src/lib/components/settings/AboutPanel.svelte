<script lang="ts">
  import { getName, getVersion } from '@tauri-apps/api/app';
  import { updater, formatBytes } from '$lib/services/updater.svelte';
  import AppLogo from '$lib/components/AppLogo.svelte';

  let appName = $state('ZNet Sink');
  let appVersion = $state('0.0.1');
  let loaded = $state(false);

  $effect(() => {
    loadAppInfo();
  });

  async function loadAppInfo() {
    try {
      const [name, ver] = await Promise.all([
        getName(),
        getVersion(),
      ]);
      appName = name;
      appVersion = ver;
    } catch {
      // running outside Tauri (browser dev), use defaults
      appName = 'ZNet Sink';
      appVersion = '0.0.1';
    }
    loaded = true;
  }

  async function handleCheckUpdate() {
    const hasUpdate = await updater.checkForUpdate();
    if (!hasUpdate) {
      // toast handled in service
    }
  }

  async function handleDownloadUpdate() {
    await updater.downloadAndInstall();
  }
</script>

<div class="about-root desk-card">
  <!-- Hero -->
  <div class="about-hero">
    <div class="about-logo">
      <AppLogo size={48} />
    </div>
    <div class="about-hero-text">
      <span class="about-hero-name">{appName}</span>
      <span class="about-hero-tagline">零域网络代理客户端</span>
    </div>
    {#if loaded}
      <span class="about-version-badge">v{appVersion}</span>
    {:else}
      <span class="about-version-badge loading">...</span>
    {/if}
  </div>

  <div class="config-separator"></div>

  <!-- App info -->
  <div class="config-section">
    <div class="config-section-title">应用信息</div>

    <div class="config-row">
      <span class="config-label">应用名称</span>
      <span class="config-value">{appName}</span>
    </div>

    <div class="config-row">
      <span class="config-label">版本号</span>
      <span class="config-value mono">v{appVersion}</span>
    </div>

    <div class="config-row">
      <span class="config-label">构建标识</span>
      <span class="config-value mono">org.zerodenet.znetsink</span>
    </div>
  </div>

  <div class="config-separator"></div>

  <!-- Resources -->
  <div class="config-section">
    <div class="config-section-title">资源</div>

    <div class="config-row">
      <span class="config-label">仓库地址</span>
      <a
        class="config-value mono link"
        href="https://github.com/zerodenet/znet-sink"
        target="_blank"
        rel="noopener noreferrer"
      >
        github.com/zerodenet/znet-sink
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" class="link-icon">
          <path d="M3 1H1v8h8V7"/>
          <path d="M6 1h3v3"/>
          <path d="M10 0L4.5 5.5"/>
        </svg>
      </a>
    </div>

    <div class="config-row">
      <span class="config-label">许可证</span>
      <span class="config-value">MIT</span>
    </div>

    <div class="config-row">
      <span class="config-label">描述</span>
      <span class="config-value desc">ZNet Sink 是一款轻量级网络代理管理客户端，提供配置管理、订阅同步、规则集编辑、实时连接监控、TUN 虚拟网卡等能力。</span>
    </div>
  </div>

  <!-- Update -->
  <div class="config-separator"></div>

  <div class="config-section">
    <div class="config-section-title">更新</div>

    {#if updater.checking}
      <div class="config-row">
        <span class="config-label">状态</span>
        <span class="config-value muted">检查中…</span>
      </div>
    {:else if updater.updateAvailable}
      <div class="update-banner">
        <div class="update-banner-header">
          <svg width="14" height="14" viewBox="0 0 10 10" fill="none" stroke="#F59E0B" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 1.5v5M2.5 4L5 6.5 7.5 4"/>
            <line x1="1" y1="9" x2="9" y2="9"/>
          </svg>
          <span class="update-banner-title">新版本可用</span>
        </div>
        <div class="update-banner-body">
          <span>v{updater.latestVersion}（当前 v{updater.currentVersion}）</span>
          {#if updater.releaseNotes}
            <span class="update-notes">{updater.releaseNotes.slice(0, 200)}{updater.releaseNotes.length > 200 ? '…' : ''}</span>
          {/if}
        </div>
        {#if updater.downloading}
          <div class="update-progress">
            <div class="update-progress-bar">
              <div
                class="update-progress-fill"
                class:indeterminate={updater.progressPct == null}
                style={updater.progressPct != null ? `width: ${updater.progressPct}%` : ''}
              ></div>
            </div>
            <span class="update-progress-text">
              {updater.progressPct != null ? `${updater.progressPct}%` : '下载中'}
              · {formatBytes(updater.downloaded)}{updater.total != null ? ` / ${formatBytes(updater.total)}` : ''}
            </span>
          </div>
        {/if}
        <button
          class="update-btn"
          onclick={handleDownloadUpdate}
          disabled={updater.downloading}
        >
          {updater.downloading ? '下载中…' : '下载并安装'}
        </button>
      </div>
    {:else}
      <div class="config-row">
        <span class="config-label">状态</span>
        <span class="config-value">已是最新</span>
      </div>
      <div class="config-row">
        <span class="config-label"></span>
        <button class="check-update-btn" onclick={handleCheckUpdate} disabled={updater.checking}>
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="1 6 3 8 7 2"/>
            <path d="M11 6A5 5 0 1 1 9.6 2.4"/>
          </svg>
          <span>检查更新</span>
        </button>
      </div>
    {/if}
  </div>

  <!-- Footer -->
  <div class="about-footer">
    <span class="about-copyright">&copy; {new Date().getFullYear()} ZeroDenet. All rights reserved.</span>
  </div>
</div>

<style>
  .about-root {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ---- Hero ---- */
  .about-hero {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 12px 14px;
  }

  .about-logo {
    flex-shrink: 0;
  }

  .about-hero-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }

  .about-hero-name {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--foreground);
    line-height: 1.3;
  }

  .about-hero-tagline {
    font-size: 11px;
    color: var(--muted-foreground);
    opacity: 0.75;
  }

  .about-version-badge {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    padding: 3px 8px;
    border-radius: 5px;
    font-size: 11px;
    font-weight: 600;
    font-family: var(--font-mono);
    background: var(--muted);
    color: var(--muted-foreground);
  }

  .about-version-badge.loading {
    opacity: 0.4;
  }

  /* ---- Shared: config helpers ---- */
  .config-separator {
    height: 1px;
    background: var(--border);
    margin: 0 12px;
  }

  .config-section {
    display: flex;
    flex-direction: column;
    padding: 8px 0;
  }

  .config-section-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--muted-foreground);
    padding: 0 12px 6px;
    opacity: 0.7;
  }

  .config-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    gap: 12px;
    transition: background 0.1s ease;
  }

  .config-row:hover {
    background: var(--muted);
  }

  .config-label {
    font-size: 12px;
    color: var(--muted-foreground);
    flex-shrink: 0;
    min-width: 72px;
  }

  .config-value {
    font-size: 12px;
    color: var(--foreground);
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 220px;
  }

  .config-value.mono {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--muted-foreground);
  }

  .config-value.desc {
    white-space: normal;
    text-align: right;
    line-height: 1.45;
    font-size: 11.5px;
    max-width: 240px;
    color: var(--muted-foreground);
  }

  .config-value.link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--primary);
    text-decoration: none;
    cursor: pointer;
    transition: opacity 0.12s ease;
  }

  .config-value.link:hover {
    opacity: 0.8;
  }

  .link-icon {
    flex-shrink: 0;
    opacity: 0.5;
  }

  .config-value.muted {
    color: var(--muted-foreground);
    opacity: 0.6;
  }

  /* ---- Update banner ---- */
  .update-banner {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 4px 12px;
    padding: 12px;
    border-radius: 8px;
    border: 1px solid rgba(245, 158, 11, 0.25);
    background: rgba(245, 158, 11, 0.06);
  }

  .update-banner-header {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .update-banner-title {
    font-size: 13px;
    font-weight: 700;
    color: #D97706;
  }

  .update-banner-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--foreground);
    line-height: 1.5;
  }

  .update-notes {
    font-size: 11px;
    color: var(--muted-foreground);
    line-height: 1.45;
    white-space: pre-line;
  }

  .update-progress {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .update-progress-bar {
    height: 4px;
    border-radius: 2px;
    background: rgba(245, 158, 11, 0.18);
    overflow: hidden;
  }

  .update-progress-fill {
    height: 100%;
    background: #D97706;
    border-radius: 2px;
    transition: width 0.2s ease;
  }

  .update-progress-fill.indeterminate {
    width: 30%;
    animation: about-indeterminate 1.2s ease-in-out infinite;
  }

  @keyframes about-indeterminate {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(400%); }
  }

  .update-progress-text {
    font-size: 11px;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    color: var(--muted-foreground);
  }

  .update-btn {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 30px;
    padding: 0 16px;
    border-radius: 7px;
    border: none;
    background: #D97706;
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.13s ease;
  }

  .update-btn:hover:not(:disabled) { opacity: 0.88; }
  .update-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .check-update-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: none;
    background: transparent;
    color: var(--primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    padding: 4px 6px;
    border-radius: 5px;
    transition: background 0.12s ease;
  }

  .check-update-btn:hover:not(:disabled) {
    background: var(--muted);
  }

  .check-update-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  /* ---- Footer ---- */
  .about-footer {
    margin-top: auto;
    padding: 10px 12px;
    border-top: 1px solid var(--border);
    text-align: center;
  }

  .about-copyright {
    font-size: 11px;
    color: var(--muted-foreground);
    opacity: 0.5;
  }
</style>
