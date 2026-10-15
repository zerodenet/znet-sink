<script lang="ts">
  // Deterministic backend fixtures only. Production Panel/session are unmodified.
  import { tick } from 'svelte';
  import PacketRoutesPanel from '../../src/lib/components/tabs/PacketRoutesPanel.svelte';
  import type { PacketGateway, PacketRoute, PacketTarget, PacketPhysicalTarget, PacketCloseRecord } from '../../src/lib/features/packet/types';
  import { physicalKey } from '../../src/lib/features/packet/types';
  import type { ClientCoreSnapshot } from '../../src/lib/types/gui-api';
  const mode = new URLSearchParams(location.search).get('mode');
  const blocked = new Map<string, PacketPhysicalTarget>();
  const records = new Map<string, PacketCloseRecord>();
  const persistent = new URLSearchParams(location.search).get('fixture') === 'packet-recovery';
  const storageKey = 'packet-host-recovery-fixture';
  let hostSessionId = 'host-session-original', currentCore = 'core-original', storageBlocked = false, serial = 0;
  let generation = 1, revision = 1, profile = 'profile-a';
  let fail = mode === 'failed-next';
  let release: (() => void) | null = null;
  let ended = false;
  let mounted = $state(true);
  let requests = $state<number[]>([]);
  let closes = $state<PacketTarget[]>([]);
  let detailTargets = $state<PacketTarget[]>([]);
  function saveLedger() { if (persistent) sessionStorage.setItem(storageKey, JSON.stringify({ targets:[...blocked],records:[...records],hostSessionId,closes,storageBlocked,currentCore,serial })); }
  if (persistent && sessionStorage.getItem(storageKey)) {
    const saved=JSON.parse(sessionStorage.getItem(storageKey)!);hostSessionId=saved.hostSessionId;storageBlocked=saved.storageBlocked;currentCore=saved.currentCore;
    closes=saved.closes;serial=saved.serial??0;for(const [key,target] of saved.targets)blocked.set(key,target);for(const [key,record] of saved.records)records.set(key,record);
  }
  function recordFor(target: PacketPhysicalTarget): PacketCloseRecord {
    return { target,operationId:`close-operation-${++serial}`,originProcessId:hostSessionId,originProfileId:profile,originClientScopeGeneration:String(generation),reservedAtUnixMs:'1000',recovered:false };
  }
  function host(): ClientCoreSnapshot { return { revision, scopeGeneration:generation, scope:{profileId:profile,configRevision:'7',coreInstanceId:1},sourceStatus:'ready',activeProbeJobs:[] }; }
  function row(index: number): PacketRoute { return { routeId:`packet:${index}`, coreInstanceId:currentCore, configRevision:'18446744073709551615', inboundTag:'tun',outboundTag:'wg',source:`2001:db8::${index+1}`,destination:'2001:db8:1::1',ipProtocol:58,translated:false,startedAtUnixMs:'18446744073709551615',state:ended&&index===0?'closed':'active',endpoints:[{endpointId:'actual-fixture-device',generation:'18446744073709551615'}] }; }
  if (mode === 'full') for (let index=0;index<128;index++) { const target={...row(index),routeId:`unknown:${index}`};blocked.set(physicalKey(target),target); records.set(physicalKey(target),recordFor(target)); }
  const gateway: PacketGateway = {
    async snapshot(query) {
      if (!query) throw new Error('production request must carry bounded query');
      requests = [...requests,query.offset];
      if(query.expected && query.expected.hostSessionId !== hostSessionId) throw {code:'conflict',message:'宿主会话已变化，请返回第一页重新检查'};
      if (query.offset===500 && fail) throw {code:'core_unavailable',message:'测试后页读取失败'};
      if (query.offset===500 && mode==='pending-next') await new Promise<void>(resolve=>release=resolve);
      const total=1001, end=Math.min(total,query.offset+query.limit);
      const unsupported=mode==='unsupported';
      return {offset:query.offset,limit:query.limit,clientScopeGeneration:generation,hostSessionId,closeRecords:[...records.values()],closeLedgerRecovery:{state:storageBlocked?'blocked':'ready',reason:storageBlocked?'关闭账本写入失败，原记录保留':null,recoveredCount:[...records.values()].filter(record=>record.recovered).length},blockedTargets:[...blocked.values()].map(target=>({routeId:target.routeId,coreInstanceId:target.coreInstanceId,configRevision:target.configRevision})),closeLedgerFull:storageBlocked||blocked.size>=128,supported:!unsupported,canClose:!unsupported&&!storageBlocked,reason:unsupported?'当前内核未声明 Packet 路径管理能力':null,closeReason:unsupported?'不支持关闭':null,profileId:profile,coreInstanceId:unsupported?null:currentCore,configRevision:unsupported?null:'18446744073709551615',observedAtUnixMs:unsupported?null:'1000',routes:unsupported?[]:Array.from({length:Math.max(0,end-query.offset)},(_,index)=>row(query.offset+index)),total:unsupported?null:total,nextOffset:!unsupported&&end<total?end:null};
    },
    async detail(target) {
      detailTargets = [...detailTargets,target];
      if(target.hostSessionId!==hostSessionId) throw {code:'conflict',message:'原宿主会话已失效'};
      const record=records.get(physicalKey(target));
      if(record && target.closeOperationId!==record.operationId) throw {code:'conflict',message:'原关闭操作已被替换'};
      if (ended) {
        if(storageBlocked) throw {code:'ledger_unavailable',message:'原会话结束事实已读取，但关闭账本持久写入失败；屏障保留'};
        blocked.delete(physicalKey(target));records.delete(physicalKey(target));saveLedger();return {...row(0),state:'closed',closeOperationId:record?.operationId??null};
      }
      return {...row(Number(target.routeId.split(':')[1])),closeOperationId:record?.operationId??null};
    },
    async close(target) {
      if(target.hostSessionId!==hostSessionId || target.closeOperationId) throw {code:'conflict',message:'宿主会话或恢复操作不允许新关闭'};
      if(storageBlocked) throw {code:'ledger_unavailable',message:'关闭账本未就绪'};
      if (blocked.has(physicalKey(target))) throw {code:'conflict',message:'host barrier'};
      blocked.set(physicalKey(target),target);const record=recordFor(target);records.set(physicalKey(target),record);
      closes = [...closes,target];saveLedger();
      return {status:'unconfirmed',message:'原关闭结果未确认；只读核对原目标',route:null,operationId:record.operationId};
    },
  };
  Object.assign(window,{packetFixture:{
    prepareHostRebuild(){hostSessionId='host-session-reconstructed';for(const [key,record] of records)records.set(key,{...record,recovered:true});saveLedger();},
    blockStorage(){storageBlocked=true;saveLedger();},
    changeCore(){currentCore='core-replaced';saveLedger();},
    replaceHost(){hostSessionId='different-live-host';},
    release(){release?.();},allowNext(){fail=false;},endOriginal(){ended=true;},
    changeScope(){generation++;revision++;profile=profile==='profile-a'?'profile-b':'profile-a';window.dispatchEvent(new CustomEvent('client-core:updated',{detail:host()}));},
    observe(){revision++;window.dispatchEvent(new CustomEvent('client-core:updated',{detail:host()}));},
    async remount(){mounted=false;await tick();mounted=true;},
  }});
</script>
{#if mounted}<PacketRoutesPanel {gateway} readHost={async()=>host()} />{/if}
<output class="sr-only" aria-label="Packet 后端请求偏移">{JSON.stringify(requests)}</output>
<output class="sr-only" aria-label="Packet 关闭提交次数">{closes.length}</output>
<output class="sr-only" aria-label="Packet 原始关闭目标">{JSON.stringify(closes)}</output>
<output class="sr-only" aria-label="Packet 详情目标">{JSON.stringify(detailTargets)}</output>
