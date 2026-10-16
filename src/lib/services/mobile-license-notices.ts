import projectText from '../../../LICENSE?raw';
import {invoke} from '@tauri-apps/api/core';
export const projectLicense={id:'MPL-2.0',text:projectText,source:'仓库 LICENSE'} as const;
import {parseLicenseInventory,type LicenseInventory,type LicenseInventoryView} from '$lib/features/mobile/license-inventory';
export {parseLicenseInventory,licenseEvidenceLabels,type LicenseInventory,type LicenseEntry,type LicenseText,type LicenseInventoryView} from '$lib/features/mobile/license-inventory';
export async function loadLicenseInventories():Promise<LicenseInventoryView> {
  const result:LicenseInventoryView={inventories:[],errors:[]};
  const fetched=await Promise.allSettled([
    fetch(`${import.meta.env.BASE_URL}license-inventory.frontend.json`).then(async r=>{if(!r.ok)throw new Error('本次前端构建许可目录未提供');const text=await r.text();if(text.length>16*1024*1024)throw new Error('许可目录超过读取预算');return parseLicenseInventory(JSON.parse(text));}),
    invoke<{schemaVersion:number;rust:unknown;native:{status:string;inventory:unknown;reason:string|null}}>('app_license_inventory').then(value=>{if(value.schemaVersion!==1)throw new Error('宿主许可目录协议未确认');return value;})
  ]);
  if(fetched[0].status==='fulfilled')result.inventories.push(fetched[0].value as LicenseInventory);else result.errors.push('本次前端构建许可目录未提供或校验失败。');
  if(fetched[1].status==='fulfilled') {
    const envelope=fetched[1].value as {rust:unknown;native:{status:string;inventory:unknown;reason:string|null}};
    try{result.inventories.push(parseLicenseInventory(envelope.rust));}catch{result.errors.push('Rust 目标许可目录未确认。');}
    if(envelope.native.status==='available'){try{result.inventories.push(parseLicenseInventory(envelope.native.inventory));}catch{result.errors.push('原生许可目录校验失败。');}}
    else result.errors.push(`原生构建许可目录未提供（${envelope.native.reason??envelope.native.status}）。`);
  }else result.errors.push('宿主构建许可目录当前不可读取。');
  return result;
}
