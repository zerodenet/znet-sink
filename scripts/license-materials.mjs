// Fetch only authoritative URLs explicitly referenced by the resolved inventory.
// Cached raw bodies and their hashes are reviewable; a license identifier is never a body.
import { readFileSync,writeFileSync,mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname,resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sha256,serializeInventory,argumentsOf } from './license-common.mjs';

/** @param {any} row @returns {string[]} */
export function declaredLicenseUrls(row) { return [...(row.licenseUrls||[]),...(row.evidence||[]).flatMap(/** @param {any} item */ item=>typeof item.detail==='string' && item.detail.startsWith('POM license URL: ')?[item.detail.slice(17)]:[])]; }
/** @param {any} inventory @param {any} cache */
export function applyMaterialCache(inventory,cache) {
  const rows=structuredClone(inventory.entries),texts=[...inventory.texts],gaps=[...inventory.gaps];
  for(const row of rows) {
    for(const material of cache.materials) {
      if(!declaredLicenseUrls(row).some(/** @param {string} url */ url=>material.declaredUrls.includes(url)))continue;
      if(sha256(Buffer.from(material.text,'utf8'))!==material.sha256)throw new Error('External license cache digest mismatch');
      const id=`sha256-${material.sha256}`;
      texts.push({id,sha256:material.sha256,name:'Declared authoritative license body',kind:'license',source:material.source,text:material.text});
      const lackedOwnBody=row.licenseTextIds.length===0;
      row.licenseTextIds=[...new Set([...row.licenseTextIds,id])].sort();
      row.textStatus='available';
      row.materialReferences=[...(row.materialReferences||[]),{id,name:'Reviewed common license terms',kind:'license',source:material.source,sha256:material.sha256}];
      row.evidence.push({kind:'license-material',detail:'Common terms from reviewed material bound to this resolved POM canonical URL; package-specific attribution/NOTICE availability remains separate.',source:material.source,sha256:material.sha256});
      if(lackedOwnBody){row.missing.push('package-specific-legal-material-not-provided');gaps.push({ecosystem:row.ecosystem,id:row.id,reason:'package-specific-legal-material-not-provided'});}
      row.missing=row.missing.filter(/** @param {string} reason */ reason=>!['license-text-not-provided-by-installed-material','license-body-missing'].includes(reason));
    }
  }
  const covered=new Set(rows.filter(/** @param {any} row */ row=>row.licenseTextIds.length).map(/** @param {any} row */ row=>row.id));
  return {...inventory,entries:rows,texts,gaps:gaps.filter(/** @param {any} gap */ gap=>!covered.has(gap.id)||!['license-text-not-provided-by-installed-material','license-body-missing'].includes(gap.reason))};
}
if(process.argv[1]===fileURLToPath(import.meta.url)) {
  const args=argumentsOf(process.argv.slice(2));
  if(!args.input||!args.output)throw new Error('--input and --output required');
  const inventory=JSON.parse(readFileSync(args.input,'utf8'));
  let cache;
  if(args.cache)cache=JSON.parse(readFileSync(args.cache,'utf8'));
  else {
    const urls=[...new Set(inventory.entries.flatMap(/** @param {any} row */ row=>declaredLicenseUrls(row)))].sort();
    const groups=new Map();
    for(const declared of urls) {
      const url=new URL(declared);
      // This narrowly supported body is immutable by license version and explicitly
      // declared by current Maven POMs. Other URLs remain gaps until reviewed.
      if(!['apache.org','www.apache.org'].includes(url.hostname)||!['/licenses/LICENSE-2.0.txt','/licenses/LICENSE-2.0'].includes(url.pathname))continue;
      const source='https://www.apache.org/licenses/LICENSE-2.0.txt';
      const group=groups.get(source)||[];group.push(declared);groups.set(source,group);
    }
    const materials=[];
    for(const [source,declaredUrls]of groups) {
      const text=execFileSync('curl',['--fail','--location','--max-time','30',source],{encoding:'utf8',maxBuffer:2*1024*1024});
      if(!text.includes('Apache License')||!text.includes('Version 2.0'))throw new Error('Declared authoritative body not recognized');
      materials.push({source,declaredUrls,sha256:sha256(Buffer.from(text,'utf8')),text});
    }
    cache={schemaVersion:1,materials};
    if(args['write-cache']){mkdirSync(dirname(resolve(args['write-cache'])),{recursive:true});writeFileSync(args['write-cache'],JSON.stringify(cache,null,2)+'\n');}
  }
  mkdirSync(dirname(resolve(args.output)),{recursive:true});writeFileSync(args.output,serializeInventory(applyMaterialCache(inventory,cache)));
}
