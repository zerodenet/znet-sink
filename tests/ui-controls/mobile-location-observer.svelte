<script lang="ts">
  import {onMount} from 'svelte';
  import {useMobileNavigation} from '$lib/features/mobile/context';
  const navigation=useMobileNavigation();
  // Test access to the real host navigation; no fixture product controls or routes.
  onMount(()=>{Object.assign(window,{mobileLocationNavigation:navigation});return()=>{delete (window as unknown as {mobileLocationNavigation?:unknown}).mobileLocationNavigation;};});
</script>
