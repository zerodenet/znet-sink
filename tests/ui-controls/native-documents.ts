export const getNativeDocumentCapabilities=async()=>({platform:'android' as const,available:true,reason:'系统文件测试适配器',minimumAndroidApi:24,adaptationState:'fixture',importMaximumBytes:2097152,settingsMaximumBytes:2097152});
export const pickNativeDocument=async()=> {
  const mode=new URLSearchParams(window.location.search).get('document');
  if(mode==='delayed') await new Promise(resolve=>setTimeout(resolve,600));
  return {operationId:'pick-1',status:mode==='read'||mode==='delayed'?'read' as const:mode==='denied'?'failed' as const:'cancelled' as const,
    content:mode==='read'||mode==='delayed'?'{"outbounds":[],"route":{"final":"direct"}}':null,bytesRead:mode==='read'?46:null,reason:mode==='denied'?'系统权限拒绝；草稿保留':null};
};
export const exportNativeSettings=async(delivery:string,expectedRevision:string)=>({operationId:expectedRevision,status:delivery==='share'?'share-accepted' as const:'saved' as const,content:null,bytesRead:null,reason:null});
export const exportNativeProfile=async()=>({operationId:'profile',status:'cancelled' as const,content:null,bytesRead:null,reason:null});
export const exportNativeDiagnostics=async()=>({operationId:'diagnostics',status:'cancelled' as const,content:null,bytesRead:null,reason:null});
export { documentReceiptMessage } from '../../src/lib/services/native-documents';
