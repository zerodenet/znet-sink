<script lang="ts">
  interface Props {
    /** Dialog title shown in the draggable header. */
    title: string;
    /** Optional subtitle below the title. */
    description?: string;
    /** Whether the dialog is visible. */
    open: boolean;
    /** Called when the dialog should close (Escape key, close button). */
    onClose: () => void;
    /** CSS width for the dialog (e.g. "min(560px, 90vw)"). */
    width?: string;
    /** If true, the close button and Escape key are disabled. */
    closeDisabled?: boolean;
    /** Extra buttons rendered left of fullscreen/close in the header. */
    headerActions?: import('svelte').Snippet;
    /** Dialog body content. */
    children: import('svelte').Snippet;
    /** Optional footer rendered at the bottom. */
    footer?: import('svelte').Snippet;
  }

  let {
    title,
    description,
    open,
    onClose,
    width = 'min(520px, 90vw)',
    closeDisabled = false,
    headerActions,
    children,
    footer,
  }: Props = $props();

  let fullscreen = $state(false);
  let pos = $state({ x: 0, y: 0 });
  let dragging = false;
  let dragAnchorX = 0;
  let dragAnchorY = 0;
  let containerEl: HTMLDivElement | undefined = $state();

  // Reset position when dialog opens
  $effect(() => {
    if (open) {
      pos = { x: 0, y: 0 };
      fullscreen = false;
    }
  });

  function handleDragStart(e: MouseEvent) {
    if (fullscreen) return;
    // Only primary mouse button
    if (e.button !== 0) return;
    dragging = true;
    dragAnchorX = e.clientX - pos.x;
    dragAnchorY = e.clientY - pos.y;
    e.preventDefault();
    window.addEventListener('mousemove', handleDragMove);
    window.addEventListener('mouseup', handleDragEnd);
  }

  function handleDragMove(e: MouseEvent) {
    if (!dragging) return;
    const viewW = window.innerWidth;
    const viewH = window.innerHeight;
    const elW = containerEl?.offsetWidth ?? 400;
    const elH = containerEl?.offsetHeight ?? 300;

    // Dialog is centered by flex in the overlay — its natural position is
    // left: (viewW - elW)/2, top: (viewH - elH)/2.
    // translate(x, y) offsets from that center, so the dialog's actual
    // screen rect is:
    //   left   = (viewW - elW)/2 + x
    //   top    = (viewH - elH)/2 + y
    //   right  = left + elW
    //   bottom = top  + elH
    const halfW = (viewW - elW) / 2;
    const halfH = (viewH - elH) / 2;

    // Margin (px) — keep at least this much of the header visible on every edge
    const MARGIN = 40;

    // Constrain: keep MARGIN px of the dialog visible on each side
    const minX = MARGIN - halfW;
    const maxX = halfW - MARGIN;
    const minY = MARGIN - halfH;
    const maxY = halfH - MARGIN;

    // If the dialog is larger than the viewport in either axis,
    // just keep it centered (min > max, so clamp to 0)
    const clampX = minX <= maxX;
    const clampY = minY <= maxY;

    let newX = e.clientX - dragAnchorX;
    let newY = e.clientY - dragAnchorY;

    if (clampX) {
      newX = Math.max(minX, Math.min(maxX, newX));
    } else {
      newX = 0;
    }
    if (clampY) {
      newY = Math.max(minY, Math.min(maxY, newY));
    } else {
      newY = 0;
    }

    pos = { x: newX, y: newY };
  }

  function handleDragEnd() {
    dragging = false;
    window.removeEventListener('mousemove', handleDragMove);
    window.removeEventListener('mouseup', handleDragEnd);
  }

  function toggleFullscreen() {
    fullscreen = !fullscreen;
    if (fullscreen) {
      pos = { x: 0, y: 0 };
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !closeDisabled) {
      onClose();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="dm-overlay"
    class:dm-overlay-fullscreen={fullscreen}
    role="presentation"
    onkeydown={handleKeydown}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      bind:this={containerEl}
      class="dm-container"
      class:dm-fullscreen={fullscreen}
      style="width: {fullscreen ? '100%' : width}; {fullscreen ? '' : `transform: translate(${pos.x}px, ${pos.y}px)`}"
      role="dialog"
      aria-modal="true"
    >
      <!-- Drag handle = header -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="dm-header"
        onmousedown={handleDragStart}
        ondblclick={toggleFullscreen}
      >
        <div class="dm-header-text">
          <div class="dm-title">{title}</div>
          {#if description}
            <div class="dm-desc">{description}</div>
          {/if}
        </div>
        <div class="dm-header-actions">
          {#if headerActions}
            {@render headerActions()}
          {/if}
          <button
            class="dm-icon-btn"
            onclick={toggleFullscreen}
            title={fullscreen ? '还原' : '全屏'}
            aria-label={fullscreen ? '还原' : '全屏'}
          >
            {#if fullscreen}
              <!-- minimize icon -->
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M9 1h4v4M5 13H1V9M13 1L9 5M1 13l4-4"/>
              </svg>
            {:else}
              <!-- maximize icon -->
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                <path d="M1 5V1h4M9 13h4V9M1 1l4 4M13 13l-4-4"/>
              </svg>
            {/if}
          </button>
          <button
            class="dm-icon-btn"
            onclick={onClose}
            disabled={closeDisabled}
            title="关闭"
            aria-label="关闭"
          >
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
              <line x1="3" y1="3" x2="11" y2="11"/><line x1="11" y1="3" x2="3" y2="11"/>
            </svg>
          </button>
        </div>
      </div>

      <div class="dm-body">
        {@render children()}
      </div>

      {#if footer}
        <div class="dm-footer">
          {@render footer()}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* ── Overlay (backdrop) ── */
  .dm-overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: var(--dialog-overlay-bg);
    animation: dm-fade-in 0.15s ease;
  }

  .dm-overlay-fullscreen {
    padding: 0;
  }

  /* ── Container (the dialog box — opaque) ── */
  .dm-container {
    position: relative;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--dialog-bg);
    box-shadow: var(--dialog-shadow);
    max-height: min(90vh, 840px);
    animation: dm-scale-in 0.15s ease;
  }

  .dm-fullscreen {
    max-height: 100% !important;
    height: 100%;
    border-radius: 0;
    border: none;
    transform: none !important;
  }

  /* ── Header (drag handle) ── */
  .dm-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px 10px;
    cursor: grab;
    user-select: none;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .dm-header:active {
    cursor: grabbing;
  }

  .dm-header-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .dm-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--foreground);
    line-height: 1.3;
  }

  .dm-desc {
    font-size: 11.5px;
    color: var(--muted-foreground);
    line-height: 1.4;
  }

  .dm-header-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  /* ── Icon buttons ── */
  .dm-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: none;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }

  .dm-icon-btn:hover:not(:disabled) {
    background: var(--muted);
    color: var(--foreground);
  }

  .dm-icon-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  /* ── Body ── */
  .dm-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* ── Footer ── */
  .dm-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 10px 14px 12px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }

  /* ── Animations ── */
  @keyframes dm-fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes dm-scale-in {
    from { opacity: 0; transform: scale(0.96); }
    to { opacity: 1; transform: scale(1); }
  }
</style>
