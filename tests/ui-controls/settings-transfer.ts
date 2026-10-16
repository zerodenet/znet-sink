import type { SettingsTransferGateway, SettingsTransferPreview } from '../../src/lib/types/settings-transfer';
const calls: string[] = [];
function report() { let output=document.querySelector('output#settings-transfer-fixture');if(!output){output=document.createElement('output');output.id='settings-transfer-fixture';document.body.append(output);}output.textContent=JSON.stringify(calls); }
const plan: SettingsTransferPreview = { planId:'native-fixed-plan',expectedRevision:'base-revision',targetRevision:'target-revision',scope:{profileId:'main',configRevision:'1',coreInstanceId:1},changedFields:['tun.mtu'],unchangedFields:[],expiresAtUnixMs:String(Date.now()+300000),runtimeRequired:true,compatibilityPending:false,notice:'仅导入固定预览范围；已确认本地保存与运行应用分别呈现。' };
const receipt={planId:plan.planId,localState:'saved' as const,runtimeState:'pending-apply' as const,savedRevision:plan.targetRevision,theme:'system',notice:'已保存本地设置，待运行应用'};
export const settingsTransferGateway: SettingsTransferGateway = {
  export:async()=>({document:'{"schemaVersion":"znet.settings-transfer.v1","clientEdits":{"tun.mtu":1400}}',omittedFields:['dns'],expectedRevision:'base-revision'}),
  preview:async content=>{
    calls.push('preview');report();
    let value: unknown;try{value=JSON.parse(content);}catch{throw{code:'invalid_argument',details:{fieldPath:'document'}};}
    if((value as {clientEdits?:Record<string,unknown>}).clientEdits?.['tun.enabled']!==undefined)throw{code:'invalid_argument',details:{fieldPath:'clientEdits'}};
    return plan;
  },
  commit:async id=>{calls.push(`commit:${id}`);report();if(new URLSearchParams(location.search).has('migration-unknown'))throw{code:'connection_closed',message:'raw-secret-token'};return receipt;},
  status:async id=>{calls.push(`status:${id}`);report();return receipt;},
};
