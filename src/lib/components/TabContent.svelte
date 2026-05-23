<script lang="ts">
  import { store } from '$lib/services/store.svelte';
  import OverviewTab from './tabs/OverviewTab.svelte';
  import NodesTab from './tabs/NodesTab.svelte';
  import ProfilesTab from './tabs/ProfilesTab.svelte';
  import SubscriptionsTab from './tabs/SubscriptionsTab.svelte';
  import RulesTab from './tabs/RulesTab.svelte';
  import LogsTab from './tabs/LogsTab.svelte';
  import ConnectionsTab from './tabs/ConnectionsTab.svelte';
  import CapabilitiesTab from './tabs/CapabilitiesTab.svelte';
  import SettingsPanel from './SettingsPanel.svelte';
  import PlaceholderTab from './tabs/PlaceholderTab.svelte';
</script>

{#if store.activeTab === 'overview'}
  <OverviewTab />
{:else if store.activeTab === 'nodes'}
  <NodesTab />
{:else if store.activeTab === 'profiles'}
  <ProfilesTab />
{:else if store.activeTab === 'subscriptions'}
  <SubscriptionsTab />
{:else if store.activeTab === 'rules'}
  <RulesTab />
{:else if store.activeTab === 'connections'}
  <ConnectionsTab />
{:else if store.activeTab === 'logs'}
  <LogsTab />
{:else if store.activeTab === 'settings'}
  <SettingsPanel />
{:else if store.activeTab === 'capabilities'}
  <CapabilitiesTab />
{:else}
  <PlaceholderTab label={store.activeTab} />
{/if}
