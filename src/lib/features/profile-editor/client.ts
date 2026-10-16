import { invoke } from '@tauri-apps/api/core';
import type { ProfileEditorInput, ProfileEditorSnapshot, ProfileEditorValidation, ProfileEditorSave } from './types';
export function getProfileEditorSnapshot(id?:string):Promise<ProfileEditorSnapshot> { return invoke('proxy_config_editor_snapshot',{id}); }
export function validateProfileEditor(input:ProfileEditorInput):Promise<ProfileEditorValidation> { return invoke('proxy_config_editor_validate',{input}); }
export function saveProfileEditor(input:ProfileEditorInput & {name:string}):Promise<ProfileEditorSave> { return invoke('proxy_config_editor_save',{input}); }
export function readProfileEditorFile(path:string):Promise<string> { return invoke('proxy_config_editor_read_file',{path}); }
