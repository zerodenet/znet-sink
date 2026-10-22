/** Config bodies cross this boundary only as text; never JavaScript Values. */
export interface ProfileEditorSnapshot {
  targetId: string|null; createTargetId:string|null; binding: string|null; name: string; sourceJson: string|null;
  readOnlyReason: string|null; sourceUpdatedAtUnixMs: string|null; validationScope: string;
}
export interface ProfileEditorInput { id?: string; binding: string; sourceJson: string }
export interface ProfileEditorValidation { state:'validated'; sourceJson:string; validationScope:string }
export interface ProfileEditorSave { state:'saved_pending_apply'; snapshot:ProfileEditorSnapshot }
export interface ProfileEditorPorts {
  snapshot(id?:string):Promise<ProfileEditorSnapshot>;
  validate(input:ProfileEditorInput):Promise<ProfileEditorValidation>;
  save(input:ProfileEditorInput & {name:string}):Promise<ProfileEditorSave>;
}
export interface FieldIssue {fieldPath:string;message:string;line?:number;column?:number}
export interface ProfileEditorView {
  phase:'idle'|'loading'|'ready'|'validating'|'saving'|'saved'|'unknown'|'error';
  id:string|null;createTargetId:string|null;binding:string|null;name:string;sourceJson:string;
  originalName:string;originalJson:string;readOnlyReason:string|null;
  validationScope:string;message:string|null;issues:FieldIssue[];readingFile:boolean;checkingLocal:boolean;localFacts:{targetId:string;name:string;updatedAtUnixMs:string|null;matchesTarget:boolean}|null;
}
