<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  let { label, name, value, onValueChange, hint = '', error = '', attempted = false, required = false, disabled = false, secret = false, placeholder = '' }: {
    label: string; name: string; value: string; onValueChange: (value: string) => void;
    hint?: string; error?: string; attempted?: boolean; required?: boolean; disabled?: boolean; secret?: boolean; placeholder?: string;
  } = $props();
  const id = $props.id();
  let touched = $state(false);
  const invalid = $derived(!!error && (attempted || touched));
</script>
<div class="grid min-w-0 gap-1.5 text-xs">
  <label for={id}>{label}{#if required} <span class="text-destructive" aria-hidden="true">*</span>{/if}</label>
  <Input id={id} {name} {value} type={secret ? 'password' : 'text'} {required} {disabled} {placeholder} autocomplete={secret ? 'new-password' : 'off'} spellcheck="false" class="font-mono" aria-invalid={invalid} aria-describedby={`${id}-hint${invalid ? ` ${id}-error` : ''}`} oninput={event => onValueChange(event.currentTarget.value)} onblur={() => touched = true} />
  <p id={`${id}-hint`} class="text-muted-foreground">{hint}</p>
  {#if invalid}<p id={`${id}-error`} class="text-destructive">{error}</p>{/if}
</div>
