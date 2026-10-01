import type { TrafficGateway, TrafficPage, TrafficQuery } from '$lib/features/traffic/types';
import { scopeKey } from '$lib/features/traffic/types';
import { errorCode } from '$lib/features/traffic/policy';
export const MAX_SCOPES=16384;
export async function inventory(gateway:TrafficGateway,limit:number,alive:()=>boolean):Promise<TrafficPage> {
  for(let attempt=0;attempt<3;attempt++) {
    try {
      let first:TrafficPage|undefined;let offset=0;const scopes:TrafficPage['scopes']=[];const keys=new Set<string>();
      for(let index=0;index<MAX_SCOPES;index++) {
        if(!alive())throw new Error('统计读取已取消');
        const query:TrafficQuery={offset,limit};
        if(first){query.expected_core_instance_id=first.core_instance_id;query.expected_config_revision=first.config_revision;query.expected_registry_revision=first.registry_revision;}
        const page=await gateway.page(query);
        if(!first)first=page;
        if(page.core_instance_id!==first.core_instance_id||page.config_revision!==first.config_revision||page.registry_revision!==first.registry_revision||page.total!==first.total)throw {code:'conflict',message:'统计库存分页期间发生变化'};
        if(page.total>MAX_SCOPES||page.scopes.length>limit)throw new Error('统计库存超出客户端有界读取上限');
        for(const row of page.scopes){const key=scopeKey(row.scope);if(keys.has(key))throw new Error('统计分页含重复身份');keys.add(key);scopes.push(row);}
        if(page.next_offset===null){if(scopes.length!==first.total)throw new Error('统计库存分页不完整');return {...first,scopes,next_offset:null};}
        if(page.next_offset!==offset+page.scopes.length||page.next_offset<=offset||page.next_offset>=first.total)throw new Error('统计分页游标无效');
        offset=page.next_offset;
      }
      throw new Error('统计分页超出有界读取上限');
    } catch(error){if(errorCode(error)!=='conflict'||attempt===2)throw error;}
  }
  throw new Error('统计库存读取失败');
}
