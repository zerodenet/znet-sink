<script lang="ts">
  import { isNestedOverlayEvent } from '$lib/services/overlay-keyboard';
  import { tick } from 'svelte';
  import { AlertTriangle } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';

  interface Props {
    open: boolean;
    title: string;
    description: string;
    confirmLabel: string;
    busyLabel?: string;
    busy?: boolean;
    destructive?: boolean;
    onConfirm: () => void | Promise<void>;
    onClose: () => void;
  }

  let {
    open,
    title,
    description,
    confirmLabel,
    busyLabel = '处理中…',
    busy = false,
    destructive = false,
    onConfirm,
    onClose,
  }: Props = $props();

  let dialogElement = $state<HTMLDivElement>();

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }

  function focusableElements(): HTMLElement[] {
    if (!dialogElement) return [];
    return Array.from(dialogElement.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ));
  }

  function close() {
    if (!busy) onClose();
  }

  $effect(() => {
    if (!open) return;

    const previouslyFocused = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;

    function handleKeydown(event: KeyboardEvent) {
      if (isNestedOverlayEvent(event)) return;
      if (event.key === 'Escape') {
        event.preventDefault();
        close();
        return;
      }
      if (event.key !== 'Tab') return;

      const focusable = focusableElements();
      if (focusable.length === 0) {
        event.preventDefault();
        dialogElement?.focus();
        return;
      }

      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    window.addEventListener('keydown', handleKeydown);
    void tick().then(() => (focusableElements()[0] ?? dialogElement)?.focus());

    return () => {
      window.removeEventListener('keydown', handleKeydown);
      previouslyFocused?.focus();
    };
  });
</script>

{#if open}
  <div
    use:portal
    class="fixed inset-0 z-[var(--layer-dialog)] flex items-center justify-center p-4"
    role="presentation"
  >
    <button data-slot="surface-button"
      type="button"
      class="absolute inset-0 size-full border-0 bg-black/35 backdrop-blur-[1px]"
      aria-label="取消操作"
      disabled={busy}
      onclick={close}
    ></button>

    <div
      bind:this={dialogElement}
      class="relative z-10 w-full max-w-[390px] rounded-xl border border-border bg-background p-5 shadow-2xl outline-none"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="action-confirm-title"
      aria-describedby="action-confirm-description"
      tabindex="-1"
    >
      <div class="flex items-start gap-3">
        <span class="mt-0.5 inline-flex size-8 shrink-0 items-center justify-center rounded-lg bg-destructive/10 text-destructive">
          <AlertTriangle class="size-4" />
        </span>
        <div class="min-w-0 flex-1">
          <h2 id="action-confirm-title" class="m-0 text-sm font-semibold text-foreground">{title}</h2>
          <p id="action-confirm-description" class="mt-1.5 text-xs leading-5 text-muted-foreground">{description}</p>
        </div>
      </div>

      <div class="mt-5 flex justify-end gap-2">
        <Button variant="outline" size="sm" disabled={busy} onclick={close}>
          取消
        </Button>
        <Button
          variant={destructive ? 'destructive' : 'default'}
          size="sm"
          disabled={busy}
          onclick={onConfirm}
        >
          {busy ? busyLabel : confirmLabel}
        </Button>
      </div>
    </div>
  </div>
{/if}
