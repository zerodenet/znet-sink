import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { join, relative, basename } from 'node:path';

/** @param {string | Uint8Array} value */
export const sha256 = value => createHash('sha256').update(value).digest('hex');
/** @param {string} ecosystem @param {string} name @param {string|null} version @param {string} source */
export const entryId = (ecosystem, name, version, source = '') => `${ecosystem}:${name}@${version || 'unresolved'}:${sha256(source).slice(0, 12)}`;
/** @param {string|null|undefined} value */
export function safeSource(value) {
  if (!value) return 'source-not-provided';
  try {
    const prefix = value.startsWith('git+') ? 'git+' : '';
    const u = new URL(prefix ? value.slice(4) : value);
    u.username = ''; u.password = '';
    for (const key of [...u.searchParams.keys()]) if (!['rev','tag','branch'].includes(key)) u.searchParams.delete(key);
    return prefix + u.toString();
  } catch { return value.replace(/(?:\/workspace|\/Users|\/home|[A-Z]:\\)[^\s]*/g, '[local-source]'); }
}
/** @param {string} directory @param {string} label @param {string|null} explicitLicenseFile @param {number} maxDepth */
export function readTextMaterials(directory, label, explicitLicenseFile = null, maxDepth = 4) {
  /** @type {any[]} */ const texts = [];
  /** @type {any[]} */ const gaps = [];
  const visited = new Set();
  /** @param {string} base @param {number} depth */
  const scan = (base, depth) => {
    if (depth > maxDepth) return;
    for (const file of readdirSync(base, { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name, 'en'))) {
      if (file.isDirectory()) {
        if (!['node_modules','.git','.tauri','target','build','.build','test','tests','examples'].includes(file.name)) scan(join(base,file.name), depth + 1);
      } else if (file.isFile() && /^(?:licen[sc]e|notice|copying|copyright|authors)(?:[._-]|$)/i.test(file.name)) add(join(base,file.name));
    }
  };
  /** @param {string} path */
  function add(path) {
    if (visited.has(path)) return;
    visited.add(path);
    const source = `${label}/${relative(directory,path).replaceAll('\\','/')}`;
    if (statSync(path).size > 2 * 1024 * 1024) { gaps.push({reason:'license-material-exceeds-budget',source}); return; }
    const bytes = readFileSync(path);
    if (bytes.includes(0)) { gaps.push({reason:'license-material-not-text',source}); return; }
    let text;
    try { text = new TextDecoder('utf-8', {fatal:true,ignoreBOM:true}).decode(bytes); }
    catch { gaps.push({reason:'license-material-not-utf8',source}); return; }
    if (!text.trim()) { gaps.push({reason:'license-material-empty',source}); return; }
    const hash = sha256(bytes);
    texts.push({id:`sha256-${hash}`,sha256:hash,name:basename(path),kind:/(?:\.spdx$)|^(notice|copyright|authors)/i.test(basename(path))?'notice':'license',source,text});
  }
  if (existsSync(directory)) scan(directory,0);
  if (explicitLicenseFile && existsSync(explicitLicenseFile)) add(explicitLicenseFile);
  return {texts,gaps};
}
/** @param {any} entry @param {{texts:any[],gaps:any[]}} materials */
export function attachMaterials(entry, materials) {
  entry.materialReferences = materials.texts.map(t=>({id:t.id,name:t.name,kind:t.kind,source:t.source,sha256:t.sha256}));
  entry.licenseTextIds = [...new Set(materials.texts.filter(t=>t.kind==='license').map(t=>t.id))].sort();
  entry.noticeTextIds = [...new Set(materials.texts.filter(t=>t.kind!=='license').map(t=>t.id))].sort();
  entry.textStatus = entry.licenseTextIds.length ? 'available' : 'missing';
  entry.missing = materials.gaps.map(g=>g.reason);
  if (!entry.licenseTextIds.length) entry.missing.push('license-text-not-provided-by-installed-material');
  if (!entry.declaredLicense) entry.missing.push('license-declaration-not-provided');
  return entry;
}
/** @param {any} inventory */
export function finalizeInventory(inventory) {
  /** @type {Map<string,any>} */ const entries = new Map();
  /** @type {Map<string,any>} */ const texts = new Map();
  for (const row of inventory.entries || []) {
    const existing = entries.get(row.id);
    if (!existing) entries.set(row.id,structuredClone(row));
    else {
      existing.evidence = [...existing.evidence,...row.evidence].filter(/** @param {any} v @param {number} i @param {any[]} all */ (v,i,all)=>all.findIndex(r=>JSON.stringify(r)===JSON.stringify(v))===i);
      for (const key of ['licenseTextIds','noticeTextIds','missing']) existing[key]=[...new Set([...(existing[key]||[]),...(row[key]||[])])].sort();
      existing.materialReferences=[...(existing.materialReferences||[]),...(row.materialReferences||[])].filter((v,i,all)=>all.findIndex(r=>JSON.stringify(r)===JSON.stringify(v))===i);
      if (existing.licenseTextIds.length) existing.textStatus='available';
    }
  }
  for (const text of inventory.texts || []) {
    if (text.sha256!==sha256(Buffer.from(text.text,'utf8')) || text.id!==`sha256-${text.sha256}`) throw new Error('License material digest mismatch');
    if (!texts.has(text.id)) texts.set(text.id,text);
  }
  const result = {...inventory,schemaVersion:1,
    entries:[...entries.values()].sort((a,b)=>a.id.localeCompare(b.id,'en')),
    texts:[...texts.values()].sort((a,b)=>a.id.localeCompare(b.id,'en')),
    gaps:(inventory.gaps||[]).sort(/** @param {any} a @param {any} b */ (a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b),'en'))};
  for (const row of result.entries) for (const id of [...(row.licenseTextIds||[]),...(row.noticeTextIds||[])]) if (!texts.has(id)) throw new Error('Missing referenced license material');
  return result;
}
/** @param {any} value */
export const serializeInventory = value => JSON.stringify(finalizeInventory(value),null,2)+'\n';
/** Verify reviewed material inputs across composed provenance before packaging.
 * @param {any} inventory @param {string} root @param {string[]} labels
 */
export function verifyMaterialInputs(inventory,root,labels) {
  /** @type {any[]} */ const inputs=[];
  /** @param {any} provenance */
  const visit=provenance=>{
    if(!provenance)return;
    inputs.push(...(provenance.inputs||[]));
    for(const child of provenance.inventories||[])visit(child);
  };
  visit(inventory.provenance);
  for(const label of labels){
    const path=join(root,label),proofs=inputs.filter(input=>input.label===label);
    if(!existsSync(path)&&!proofs.length)continue;
    if(!existsSync(path)||!proofs.length||proofs.some(proof=>proof.sha256!==sha256(readFileSync(path))))throw new Error(`Stale or missing package material input: ${label}`);
  }
}
/** @param {string[]} argv @returns {Record<string,string>} */
export function argumentsOf(argv) {
  /** @type {Record<string,string>} */ const result={};
  for (let i=0;i<argv.length;i++) {
    const key=argv[i];
    if (!key.startsWith('--') || !argv[i+1] || argv[i+1].startsWith('--')) throw new Error(`Invalid argument ${key}`);
    result[key.slice(2)]=argv[++i];
  }
  return result;
}
