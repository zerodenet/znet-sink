<script lang="ts">
  /**
   * AppLogo — theme-aware application icon.
   *
   * Renders the dark-mode icon (app-icon.png) when the OS/ theme is dark,
   * and the light-mode icon (app-icon-bg.png) when light.
   * Both <img> elements are always in the DOM; visibility is toggled via CSS
   * based on the `.dark` class on <html>, so no JS reactivity or flicker.
   */

  interface Props {
    /** Width & height in px (default 48) */
    size?: number;
    /** Extra CSS classes */
    class?: string;
    /** img alt text */
    alt?: string;
  }

  let { size = 48, class: className = '', alt = 'ZNet Sink' }: Props = $props();
</script>

<div
  class="app-logo-wrap {className}"
  style="width: {size}px; height: {size}px;"
>
  <!-- Light mode icon (shown when NOT dark) -->
  <img
    src="/app-icon-bg.png"
    {alt}
    class="app-logo-img light"
    width={size}
    height={size}
  />
  <!-- Dark mode icon (shown when dark) -->
  <img
    src="/app-icon.png"
    {alt}
    class="app-logo-img dark"
    width={size}
    height={size}
  />
</div>

<style>
  .app-logo-wrap {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .app-logo-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border-radius: var(--logo-radius, 8px);
    display: block;
    object-fit: contain;
    transition: opacity 0.15s ease;
  }

  /* Default (light mode): show light icon, hide dark icon */
  .app-logo-img.light {
    opacity: 1;
  }
  .app-logo-img.dark {
    opacity: 0;
  }

  /* Dark mode: show dark icon, hide light icon */
  :global(.dark) .app-logo-img.light {
    opacity: 0;
  }
  :global(.dark) .app-logo-img.dark {
    opacity: 1;
  }
</style>
