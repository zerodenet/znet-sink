import { readFileSync, realpathSync, existsSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sha256, entryId, safeSource, readTextMaterials, attachMaterials, serializeInventory } from './license-common.mjs';
import { resolveProduct } from './product-composition.mjs';
const repo = fileURLToPath(new URL('../',import.meta.url));

/** @param {string} id */
export function npmOwner(id) {
  const clean=id.replace(/^\/\@fs\//,'/').split('?')[0];
  if (clean.includes('\0') || !clean.replaceAll('\\','/').includes('/node_modules/') || !existsSync(clean)) return null;
  let cursor=dirname(realpathSync(clean));
  while (cursor.includes('node_modules')) {
    const manifest=join(cursor,'package.json');
    if (existsSync(manifest)) {
      const value=JSON.parse(readFileSync(manifest,'utf8'));
      if (value.name && value.version) return {directory:cursor,manifest,value,file:realpathSync(clean)};
    }
    const parent=dirname(cursor);if(parent===cursor)break;cursor=parent;
  }
  return null;
}
/** @param {any} owner @param {any[]} evidence */
export function packageEntry(owner, evidence) {
  const m=owner.value;
  const source=safeSource(typeof m.repository==='string'?m.repository:m.repository?.url || `npm:${m.name}@${m.version}`);
  const declaredLicense=typeof m.license==='string'?m.license:m.license||m.licenses?JSON.stringify(m.license||m.licenses):null;
  const label=`npm:${m.name}@${m.version}`;
  const materials=readTextMaterials(owner.directory,label);
  const entry=attachMaterials({id:entryId('npm',m.name,m.version,source),ecosystem:'npm',name:m.name,version:m.version,source,declaredLicense,
    evidence,manifestSha256:sha256(readFileSync(owner.manifest))},materials);
  return {entry,materials};
}
/** Actual frontend build observer; installed packages alone never imply delivery.
 * @returns {import("vite").Plugin} */
export function licenseInventory() {
  /** @type {Map<string,{owner:any,inputs:Map<string,string>,rendered:Set<string>}>} */ const observed=new Map();
  /** @type {import("vite").ResolvedConfig|undefined} */ let config;
  return {
    name:'znet-license-inventory',apply:'build',
    configResolved(value){config=value;},
    transform(_code,id) {
      if (this.environment?.config.build.ssr || this.environment?.name==='ssr' || config?.build.ssr) return;
      const owner=npmOwner(id);if(!owner)return;
      const key=owner.manifest;
      const record=observed.get(key)||{owner,inputs:new Map(),rendered:new Set()};
      record.inputs.set(relative(owner.directory,owner.file).replaceAll('\\','/'),sha256(readFileSync(owner.file)));
      observed.set(key,record);
    },
    generateBundle(_options,bundle) {
      if (this.environment?.config.build.ssr || this.environment?.name==='ssr' || config?.build.ssr) return;
      for (const item of Object.values(bundle)) {
        if (item.type!=='chunk')continue;
        for(const [id,info] of Object.entries(item.modules)) {
          const owner=npmOwner(id);if(!owner)continue;
          const record=observed.get(owner.manifest)||{owner,inputs:new Map(),rendered:new Set()};
          if (info.renderedLength>0) record.rendered.add(item.fileName);
          observed.set(owner.manifest,record);
        }
      }
      /** @type {any[]} */ const entries=[];
      /** @type {any[]} */ const texts=[];
      /** @type {any[]} */ const gaps=[];
      for(const {owner,inputs,rendered} of observed.values()) {
        const proof=[{kind:'build-input',detail:'Observed by this production client transform/module graph; not all inputs survive tree shaking.'}];
        if(rendered.size)proof.push({kind:'bundled',detail:`Nonzero rendered JavaScript in ${[...rendered].sort().join(', ')}`});
        const {entry,materials}=packageEntry(owner,proof);
        entry.observedInputSha256=sha256(JSON.stringify([...inputs].sort((a,b)=>a[0].localeCompare(b[0],'en'))));
        entries.push(entry);texts.push(...materials.texts);
        for(const reason of entry.missing)gaps.push({ecosystem:'npm',id:entry.id,reason});
      }
      // Build tool configuration imports are explicitly distinguished from client payloads.
      const cfg=readFileSync(join(repo,'vite.config.js'),'utf8');
      for(const match of cfg.matchAll(/from\s+['"]([^.'"][^'"]*)['"]/g)) {
        const name=match[1].startsWith('@')?match[1].split('/').slice(0,2).join('/'):match[1].split('/')[0];
        const path=join(repo,'node_modules',name,'package.json');
        if(!existsSync(path))continue;
        const owner={manifest:realpathSync(path),directory:dirname(realpathSync(path)),value:JSON.parse(readFileSync(path,'utf8'))};
        const {entry,materials}=packageEntry(owner,[{kind:'build-tool',detail:'Imported by the evaluated Vite configuration; not a claim of runtime bundling.'}]);
        entries.push(entry);texts.push(...materials.texts);
      }
      const rules=JSON.parse(readFileSync(join(repo,'src-tauri/resources/builtin-rules/manifest.json'),'utf8'));
      const ruleMaterials=readTextMaterials(join(repo,'src-tauri/resources/builtin-rules'),'resource:MetaCubeX/meta-rules-dat');
      entries.push(attachMaterials({id:entryId('resource','MetaCubeX/meta-rules-dat',rules.sourceCommit),ecosystem:'resource',name:'MetaCubeX/meta-rules-dat',version:rules.sourceCommit,
        declaredLicense:rules.sourceLicense,source:'https://github.com/MetaCubeX/meta-rules-dat',evidence:[{kind:'build-input',detail:'Project vendored rules; final native resource inclusion is recorded by platform packaging.'}]},ruleMaterials));
      texts.push(...ruleMaterials.texts);
      const inventory={schemaVersion:1,target:{platform:'frontend',triple:'browser',profile:'production',features:resolveProduct().features},
        provenance:{kind:'vite-client-build',inputs:[{label:'pnpm-lock.yaml',sha256:sha256(readFileSync(join(repo,'pnpm-lock.yaml')))},{label:'vite.config.js',sha256:sha256(Buffer.from(cfg))}]},entries,texts,gaps};
      this.emitFile({type:'asset',fileName:'license-inventory.frontend.json',source:serializeInventory(inventory)});
    }
  };
}
