<script lang="ts">
  import MobileLocalData from '$lib/components/mobile/MobileLocalData.svelte';
  const controls = {fail:false, calls:[] as string[]};
  Object.assign(window, {localDataFixture:controls});
  const host = (window as any).__TAURI_INTERNALS__ ?? {};
  host.invoke = async (command: string, args: any) => {
    controls.calls.push(command === 'gui_debug_clear' ? `${command}:${args.scope}` : command);
    if (command === 'gui_local_data_summary') {
      if (controls.fail) throw Error('SECRET_TOKEN_AND_PATH');
      return {sampledAtUnixMs:'1000',stores:['logs','history','capture'].map(kind => ({kind,state:kind === 'capture' ? 'unavailable' : 'sampled',
        bytes:kind === 'capture' ? null : kind === 'logs' ? '0' : '18446744073709551615',files:kind === 'capture' ? null : 1,
        byteLimit:kind === 'history' ? '33554432' : null,recordLimit:kind === 'history' ? 10000 : null,
        maxAgeMs:kind === 'history' ? '2592000000' : null,enforcement:kind === 'history' ? 'append-and-read-rotation' : 'append-rotation',path:'SECRET_PATH'}))};
    }
    if(command === 'gui_debug_clear') return;
    throw Error('Unexpected fixture operation');
  };
  Object.assign(window, {__TAURI_INTERNALS__:host});
</script>
<MobileLocalData />
