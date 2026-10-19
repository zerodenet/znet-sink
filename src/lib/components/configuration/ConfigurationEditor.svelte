<script lang="ts">
  import ProfileMaterials from './ProfileMaterials.svelte';
  let { profileId = null, onsaved, onclose }: {
    profileId?: string | null; onsaved: () => Promise<void>; onclose: () => void;
  } = $props();
</script>
<ProfileMaterials editorOnly initialProfileId={profileId} {onsaved} onclosed={onclose} />
