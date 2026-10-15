export type LicenseText={id:string;sha256:string;text:string;name:string;source:string;kind:string};
export type ReviewedLicenseMapping={expression:'MIT';basis:'reviewed-package-license-text';manifestDeclared:false;materialSha256:string};
export type LicenseEntry={id:string;name:string;version:string|null;ecosystem:string;declaredLicense:string|null;reviewedLicenseMapping?:ReviewedLicenseMapping;source:string;evidence:{kind:string;detail:string}[];licenseTextIds:string[];noticeTextIds:string[];textStatus:string;missing:string[];materialReferences?:{id:string;name:string;source:string;sha256:string;kind:string}[]};
export type LicenseInventory={schemaVersion:1;target:{platform:string;triple:string;profile:string;features:string[]};provenance:{kind:string};entries:LicenseEntry[];texts:LicenseText[];gaps:{reason:string}[]};
export type LicenseInventoryView={inventories:LicenseInventory[];errors:string[]};
/** Generated records are displayed as plain text; material identities must resolve. */
export function parseLicenseInventory(value:unknown):LicenseInventory {
  const j=value as LicenseInventory;
  if(!j||j.schemaVersion!==1||!j.target||typeof j.target.platform!=='string'||typeof j.target.triple!=='string'||typeof j.target.profile!=='string'||!Array.isArray(j.target.features)||!j.provenance||typeof j.provenance.kind!=='string'||!Array.isArray(j.entries)||!Array.isArray(j.texts)||!Array.isArray(j.gaps))throw new Error('许可目录格式未确认');
  const ids=new Set<string>();
  for(const t of j.texts){if(typeof t.text!=='string'||typeof t.source!=='string'||typeof t.name!=='string'||!/^sha256-[a-f0-9]{64}$/.test(t.id)||t.id!==`sha256-${t.sha256}`||ids.has(t.id))throw new Error('许可正文身份未确认');ids.add(t.id);}
  for(const r of j.entries){if(typeof r.id!=='string'||typeof r.name!=='string'||typeof r.source!=='string'||!Array.isArray(r.evidence)||!r.evidence.every(e=>typeof e.kind==='string'&&typeof e.detail==='string')||!Array.isArray(r.missing)||!Array.isArray(r.licenseTextIds)||!Array.isArray(r.noticeTextIds)||[...r.licenseTextIds,...r.noticeTextIds].some(id=>!ids.has(id)))throw new Error('许可资料引用未确认');
    const mapping=r.reviewedLicenseMapping;
    if(mapping!==undefined&&(!mapping||mapping.expression!=='MIT'||mapping.basis!=='reviewed-package-license-text'||mapping.manifestDeclared!==false||r.declaredLicense!=null||!r.missing.includes('license-declaration-not-provided')||typeof mapping.materialSha256!=='string'||!/^[a-f0-9]{64}$/.test(mapping.materialSha256)||!r.licenseTextIds.includes(`sha256-${mapping.materialSha256}`)))throw new Error('许可正文映射来源未确认');
  }
  return j;
}
export const licenseEvidenceLabels:Record<string,string>={bundled:'前端产物包含',compiled:'已观察编译输出', 'resolved-target':'Rust 目标解析', 'resolved-runtime':'Android 运行依赖解析','build-input':'实际构建输入','build-tool':'构建工具','runtime-provided':'系统提供','license-material':'许可正文来源','reviewed-license-mapping':'已审阅正文映射（非 manifest 声明）'};
