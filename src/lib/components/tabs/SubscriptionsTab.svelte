<script lang="ts">
  import ProfilesTab from './ProfilesTab.svelte';
  import SubscriptionSources from '$lib/components/configuration/SubscriptionSources.svelte';
  let { mobile = false }: { mobile?: boolean } = $props();
</script>
{#if mobile}<SubscriptionSources mobile />{:else}<ProfilesTab />{/if}
