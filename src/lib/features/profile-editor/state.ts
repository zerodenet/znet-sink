import type { ProfileEditorPorts, ProfileEditorSnapshot, ProfileEditorView, FieldIssue } from '$lib/features/profile-editor/types';
const initial = (): ProfileEditorView => ({phase:'idle',id:null,createTargetId:null,binding:null,name:'',sourceJson:'',originalName:'',originalJson:'',readOnlyReason:null,validationScope:'',message:null,issues:[],readingFile:false,checkingLocal:false,localFacts:null});
const nonempty=(value:unknown):value is string=>typeof value==='string' && value.length>0;
const u64=(value:unknown):value is string=>typeof value==='string' && /^(0|[1-9]\d{0,19})$/.test(value) && BigInt(value)<=18446744073709551615n;
function snapshot(value:ProfileEditorSnapshot, expected:string|null):ProfileEditorSnapshot {
  if (!value || value.targetId!==expected || (value.targetId!==null && !nonempty(value.targetId)) || (value.createTargetId!==null && !nonempty(value.createTargetId)) || (expected!==null && value.createTargetId!==null) || (expected===null && !nonempty(value.createTargetId)) || typeof value.name!=='string' || (value.sourceJson!==null && typeof value.sourceJson!=='string') || (value.binding!==null && !nonempty(value.binding)) || (value.readOnlyReason!==null && typeof value.readOnlyReason!=='string') || (value.sourceUpdatedAtUnixMs!==null && !u64(value.sourceUpdatedAtUnixMs)) || typeof value.validationScope!=='string') throw Error('invalid-editor-snapshot');
  return value;
}
function fieldIssues(error:unknown):FieldIssue[] {
  const values=(error as {details?:{errors?:unknown}}|null)?.details?.errors;
  if (!Array.isArray(values)) return [];
  return values.slice(0,32).flatMap(value=>{
    if (!value || typeof value!=='object') return [];
    const v=value as {field_path?:unknown;reasonCode?:unknown;line?:unknown;column?:unknown};
    const paths=['schema_version','inbounds','outbounds','outbound_groups','runtime','api','mode','runtime.dns','route.rule_sets','route.geoip_database','route'];
    const path=typeof v.field_path==='string' && paths.includes(v.field_path) ? v.field_path : 'config';
    const number=(n:unknown)=>typeof n==='number' && Number.isSafeInteger(n) && n>0 ? n : undefined;
    return [{fieldPath:path,message:v.reasonCode==='dependency_authorization_required'?'规则/GeoIP文件依赖缺少授权，未读取，草稿保留':v.reasonCode==='text_budget_exceeded'?'配置原文超过读取预算，未继续处理，草稿保留':'此字段未通过配置校验',line:number(v.line),column:number(v.column)}];
  });
}
function code(error:unknown):string|undefined { return error && typeof error==='object' ? (error as {code?:string}).code : undefined; }
export class ProfileEditor {
  view=initial();
  private epoch=0;private edit=0;private alive=true;
  private ports:ProfileEditorPorts;private publish:(view:ProfileEditorView)=>void;private lateSave:()=>void;
  constructor(ports:ProfileEditorPorts,publish:(view:ProfileEditorView)=>void,lateSave:()=>void=()=>{}) {this.ports=ports;this.publish=publish;this.lateSave=lateSave;}
  private emit(){if(this.alive)this.publish({...this.view});}
  get dirty(){return this.view.name!==this.view.originalName || this.view.sourceJson!==this.view.originalJson;}
  private current(epoch:number,edit:number){return this.alive && this.epoch===epoch && this.edit===edit;}
  private accept(value:ProfileEditorSnapshot){
    this.view.id=value.targetId;this.view.createTargetId=value.createTargetId;this.view.binding=value.binding;this.view.name=value.name;this.view.sourceJson=value.sourceJson??(value.targetId===null && value.binding && !value.readOnlyReason ? '{}' : '');
    this.view.originalName=this.view.name;this.view.originalJson=this.view.sourceJson;
    this.view.readOnlyReason=value.readOnlyReason??(!value.binding || (value.sourceJson===null && value.targetId!==null) ? '编辑绑定或正文未提供，不能保存；请重新读取来源' : null);
    this.view.validationScope=value.validationScope;
  }
  async open(id?:string){
    const epoch=++this.epoch;this.edit++;this.view=initial();this.view.phase='loading';this.view.id=id??null;this.emit();
    try{const value=snapshot(await this.ports.snapshot(id),id??null);if(!this.alive || this.epoch!==epoch)return;this.accept(value);this.view.phase='ready';}
    catch{if(!this.alive || this.epoch!==epoch)return;this.view.phase='error';this.view.message='读取配置原文失败；未使用列表缓存正文，请重试。';}
    this.emit();
  }
  update(field:'name'|'sourceJson',value:string){
    if(this.view.readOnlyReason || this.view.phase==='loading')return;
    this.edit++;this.view[field]=value;this.view.issues=[];
    if(!['saving','unknown','validating'].includes(this.view.phase))this.view.phase='ready';
    this.view.message=this.view.phase==='unknown'?'原保存结果未确认；草稿保留，不能重复提交。':null;this.emit();
  }
  private input(){if(!this.view.binding || this.view.readOnlyReason || ['loading','saving','validating','unknown','error'].includes(this.view.phase))return null;return {id:this.view.id??undefined,binding:this.view.binding,sourceJson:this.view.sourceJson};}
  async validate(format=false){
    const input=this.input();if(!input)return;
    const epoch=this.epoch,edit=this.edit;this.view.phase='validating';this.view.message=null;this.view.issues=[];this.emit();
    try{const result=await this.ports.validate(input);if(!this.current(epoch,edit))return;
      if(result.state!=='validated' || typeof result.sourceJson!=='string' || typeof result.validationScope!=='string')throw Error('invalid-validation');
      if(format && result.sourceJson!==this.view.sourceJson){this.edit++;this.view.sourceJson=result.sourceJson;}
      this.view.validationScope=result.validationScope;this.view.phase='ready';this.view.message=format?'已校验并格式化；尚未保存或应用。':'校验通过；尚未保存或应用。';
    }catch(error){if(!this.current(epoch,edit))return;this.view.phase='ready';this.view.issues=fieldIssues(error);this.view.message='配置校验未通过；原文和草稿保留，请修正后重试。';}
    finally{if(this.alive && this.epoch===epoch && this.view.phase==='validating')this.view.phase='ready';this.emit();}
  }
  async readText(read:()=>Promise<string|null>){
    if(this.view.readingFile || this.view.readOnlyReason || ['loading','saving','validating','unknown'].includes(this.view.phase))return;
    const epoch=this.epoch,edit=this.edit;this.view.readingFile=true;this.emit();
    try{const text=await read();if(!this.current(epoch,edit)){if(this.alive && this.epoch===epoch)this.view.message='草稿已变化，文件结果未写入；请保留修改后重新选择文件。';return;}if(text!==null){if(typeof text!=='string')throw Error('invalid-file');this.edit++;this.view.sourceJson=text;this.view.phase='ready';this.view.issues=[];this.view.message='文件原文已读取；请校验后保存，尚未应用。';}else this.view.message='已取消文件选择，草稿保留。';}
    catch(error){if(this.current(epoch,edit))this.view.message=code(error)==='document_rejected'?'系统文件操作被拒绝或失败，原草稿保留。':code(error)==='document_unconfirmed'?'系统文件操作结果未确认，原草稿保留；不会自动重试。':'系统文件读取未完成，原草稿保留；可继续粘贴 JSON。';}
    finally{if(this.alive && this.epoch===epoch)this.view.readingFile=false;this.emit();}
  }
  async save(){
    const input=this.input();if(!input || !this.view.name.trim())return false;
    const epoch=this.epoch,edit=this.edit;const expectedTarget=this.view.id??this.view.createTargetId;const name=this.view.name.trim();this.view.phase='saving';this.view.message=null;this.view.issues=[];this.emit();
    try{const result=await this.ports.save({...input,name});
      if(!this.current(epoch,edit)){if(this.alive){if(this.epoch===epoch && this.view.phase==='saving'){this.view.phase='unknown';this.view.message='原保存结果已返回，但当前草稿已变化；请核对本地版本，不能重复提交。';}this.lateSave();}return false;}
      if(result.state!=='saved_pending_apply' || !expectedTarget)throw Error('invalid-save');const saved=snapshot(result.snapshot,expectedTarget);if(!nonempty(saved.binding) || typeof saved.sourceJson!=='string' || saved.readOnlyReason!==null || !u64(saved.sourceUpdatedAtUnixMs))throw Error('invalid-save');this.accept(saved);this.view.phase='saved';this.view.message='已保存，待应用；本地保存不代表内核或 VPN 生效。';return true;
    }catch(error){if(!this.current(epoch,edit)){if(this.alive){if(this.epoch===epoch && this.view.phase==='saving'){this.view.phase='unknown';this.view.message='原保存结果已返回，但当前草稿已变化；请核对本地版本，不能重复提交。';}this.lateSave();}return false;}
      const rejected=['invalid_argument','native_validation_failed','profile_editor_conflict','profile_editor_read_only','mode_restricted','permission_denied'].includes(code(error)??'');
      this.view.phase=rejected?'ready':'unknown';this.view.issues=fieldIssues(error);this.view.message=code(error)==='profile_editor_conflict'?'来源已变化；草稿保留，请明确重新读取后编辑。':rejected?'保存被拒绝，原文和草稿保留；请修正后重试。':'本地保存结果未确认；草稿保留，不会重复提交。请只读核对本地版本。';return false;
    }finally{this.emit();}
  }
  async checkLocal(){
    if(this.view.phase!=='unknown' || this.view.checkingLocal)return;
    const target=this.view.id??this.view.createTargetId;
    if(!target){this.view.message='原保存对象未提供固定身份；请返回配置列表核对，本次不会重放保存。';this.emit();return;}
    const epoch=this.epoch,edit=this.edit;this.view.checkingLocal=true;this.emit();
    try{const value=snapshot(await this.ports.snapshot(target),target);if(!this.current(epoch,edit))return;
      this.view.localFacts={targetId:value.targetId!,name:value.name,updatedAtUnixMs:value.sourceUpdatedAtUnixMs,matchesTarget:value.targetId===target};
      this.view.message='已读取原目标的本地版本元资料；原保存结果仍未确认，草稿保留。此检查不确认运行生效，不会自动重放。';}
    catch{if(this.current(epoch,edit))this.view.message='原目标本地版本仍无法核对；原保存结果未确认，草稿保留。';}
    finally{if(this.alive && this.epoch===epoch)this.view.checkingLocal=false;this.emit();}
  }
  close(){this.epoch++;this.edit++;this.view=initial();this.emit();}
  dispose(){this.alive=false;this.epoch++;}
}
