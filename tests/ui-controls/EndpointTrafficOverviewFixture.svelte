<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { useMobileNavigation } from '$lib/features/mobile/context';
  import { useTrafficWorkspace } from '$lib/features/traffic/context';
  const navigation = useMobileNavigation();
  const workspace = useTrafficWorkspace();
  function open() {
    workspace.statistics = {kind:'global', plane:'flow', query:'', offset:0};
    navigation.open('traffic');
  }
</script>
<p>端点统计返回测试 · 模拟数据</p>
<Button onclick={open}>打开全局统计</Button>
