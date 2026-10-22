import type {SubscriptionProfile} from '$lib/types/domain';
type UsageInput=Pick<SubscriptionProfile,'usedBytes'|'uploadBytes'|'downloadBytes'|'totalBytes'>;
const counter=(value:unknown):number|null=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=0?value:null;
/** Missing directions do not establish an aggregate, and unsafe DTO numbers
 * must never be rounded into an executable or apparently measured value. */
export function subscriptionUsage(input:UsageInput){
  const upload=counter(input.uploadBytes),download=counter(input.downloadBytes),total=counter(input.totalBytes);
  let used=counter(input.usedBytes);
  let invalid=[input.usedBytes,input.uploadBytes,input.downloadBytes,input.totalBytes].some(value=>value!=null&&counter(value)===null);
  if(used===null&&input.usedBytes==null&&upload!==null&&download!==null){
    const sum=upload+download;if(Number.isSafeInteger(sum))used=sum;else invalid=true;
  }
  return {used,upload,download,total,invalid,aggregate:input.usedBytes!=null,
    percent:used!==null&&total!==null&&total>0?Math.min(100,used/total*100):null,
    overQuota:used!==null&&total!==null&&used>total};
}

/** Legacy lastError is free text. Display only fixed phase summaries, never
 * provider URLs, response fragments or credential-bearing exception text. */
export function subscriptionFailureSummary(error:string|null|undefined):{unknown:boolean;message:string}|null{
  if(!error)return null;
  if(/未确认|结果未知/.test(error))return {unknown:true,message:'订阅结果未确认；请先核对已保存材料与运行状态，不要重复提交。'};
  const status=/^subscription server returned HTTP (\d{3})$/.exec(error);
  if(status)return {unknown:false,message:`来源返回 HTTP ${status[1]}；旧确认材料已保留。`};
  if(error.includes('超时'))return {unknown:false,message:'来源读取超时；请检查连接后重试。'};
  if(error.includes('访问策略拒绝'))return {unknown:false,message:'来源读取未获授权；请检查地址与访问策略。'};
  if(/条件请求记录|本地存储/.test(error))return {unknown:false,message:'条件同步记录不可用；请核对本地存储，已有材料保留。'};
  if(/校验|解析|UTF-8/.test(error))return {unknown:false,message:'来源材料校验未通过；请修正来源后重新同步。'};
  return {unknown:false,message:'订阅同步未完成；请核对来源、已保存材料与运行状态。'};
}
