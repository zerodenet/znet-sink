<script lang="ts">
  import { flagTextParts } from '$lib/services/flag-text';
  let { text }: { text: string } = $props();
  const parts = $derived(flagTextParts(text));
</script>

{#each parts as part}
  {#if part.countryCode}<span class="inline-flag fi fi-{part.countryCode}" role="img" aria-label={part.text}></span>{:else}{part.text}{/if}
{/each}

<style>
  .inline-flag { display:inline-block; width:1.25em; height:.9375em; vertical-align:-.08em; border-radius:1px; }
</style>
