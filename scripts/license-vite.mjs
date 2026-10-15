import { readFileSync, realpathSync, existsSync } from 'node:fs';
import { dirname, join, relative, extname, resolve, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sha256, entryId, safeSource, readTextMaterials, attachMaterials, serializeInventory } from './license-common.mjs';
import { applyPackageMaterialCache } from './license-materials.mjs';
import { resolveProduct } from './product-composition.mjs';
const repo = fileURLToPath(new URL('../',import.meta.url));

/** @param {string} id */
export function npmOwner(id) {
  const clean=id.replace(/^\/\@fs\//,'/').split('?')[0];
  if (clean.includes('\0') || !resolve(clean).replaceAll('\\','/').includes('/node_modules/') || !existsSync(clean)) return null;
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
/** Exact pinned package resolution from supported pnpm v9 lockfile packages blocks.
 * No range matching, snapshot peer suffix guessing or registry access at build time.
 * @param {string} lock @param {string} name @param {string} version */
export function lockedNpmIntegrity(lock, name, version) {
  if (!/^lockfileVersion: ['"]?9\.0['"]?\s*$/m.test(lock)) return null;
  const section = lock.split(/^packages:\s*$/m)[1]?.split(/^snapshots:\s*$/m)[0];
  if (!section) return null;
  const key = `${name}@${version}`;
  const blocks = section.split(/(?=^  \S)/m).filter(block => {
    const header = block.split('\n', 1)[0].trim();
    return header === `${key}:` || header === `'${key}':` || header === `"${key}":`;
  });
  if (blocks.length !== 1) return null;
  return /^    resolution: \{integrity: (sha(?:256|384|512)-[A-Za-z0-9+/]+={0,2})\}\s*$/m.exec(blocks[0])?.[1] ?? null;
}
/** @param {any} owner @param {any[]} evidence */
export function packageEntry(owner, evidence, lock = readFileSync(join(repo,'pnpm-lock.yaml'),'utf8')) {
  const m=owner.value;
  const source=safeSource(typeof m.repository==='string'?m.repository:m.repository?.url || `npm:${m.name}@${m.version}`);
  const declaredLicense=typeof m.license==='string'?m.license:m.license||m.licenses?JSON.stringify(m.license||m.licenses):null;
  const label=`npm:${m.name}@${m.version}`;
  const materials=readTextMaterials(owner.directory,label);
  const lockIntegrity = lockedNpmIntegrity(lock, m.name, m.version);
  const entry=attachMaterials({id:entryId('npm',m.name,m.version,source),ecosystem:'npm',name:m.name,version:m.version,source,declaredLicense,
    evidence,manifestSha256:sha256(readFileSync(owner.manifest)),lockIntegrity},materials);
  if (!lockIntegrity) entry.missing.push('npm-exact-lock-integrity-unavailable');
  return {entry,materials};
}
/** Actual frontend build observer; installed packages alone never imply delivery.
 * @returns {import("vite").Plugin} */
export function licenseInventory() {
  /** @type {Map<string,{owner:any,inputs:Map<string,string>,rendered:Set<string>}>} */ const observed=new Map();
  /** @type {import("vite").ResolvedConfig|undefined} */ let config;
  /** @type {Set<string>} */ const cssBuildInputs = new Set();
  let cssObserverExpected = false, cssObserverAttached = false;
  return {
    name:'znet-license-inventory',apply:'build',
    configResolved(value) {
      config = value;
      cssObserverExpected = value.plugins.some(plugin=>plugin.name.startsWith('@tailwindcss/vite:'));
      // Vite 8/Rolldown does not expose Rollup's getWatchFiles. Observe the
      // CSS compiler's actual dependency registration, forwarding unchanged.
      for (const plugin of value.plugins.filter(plugin=>plugin.name === '@tailwindcss/vite:generate:build')) {
        const hook = plugin.transform;
        if (!hook) continue;
        cssObserverAttached = true;
        const handler = typeof hook === 'function' ? hook : hook.handler;
        /** @this {any} @param {...any} args */
        const observe = function (...args) {
          const originalContext = this;
          const context = new Proxy(originalContext, {get(target, property) {
            const original = Reflect.get(target, property, target);
            if (property === 'addWatchFile') return /** @param {string} file */ file => {
              if (!(originalContext.environment?.config.build.ssr || originalContext.environment?.name === 'ssr')) cssBuildInputs.add(file);
              return Reflect.apply(original, originalContext, [file]);
            };
            return typeof original === 'function' ? original.bind(target) : original;
          }});
          return Reflect.apply(handler, context, args);
        };
        plugin.transform = typeof hook === 'function' ? observe : {...hook,handler:observe};
      }
    },
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
      // Tailwind inlines CSS dependencies before Vite's transform/module graph.
      // Only files registered by the actual build are admitted; node_modules is never enumerated.
      for (const file of cssBuildInputs) {
        if (!/\.(?:css|woff2?|ttf|otf|svg)$/i.test(file.split('?')[0])) continue;
        const owner = npmOwner(file); if (!owner) continue;
        const record = observed.get(owner.manifest) || {owner,inputs:new Map(),rendered:new Set()};
        record.inputs.set(relative(owner.directory,owner.file).replaceAll('\\','/'),sha256(readFileSync(owner.file)));
        observed.set(owner.manifest,record);
      }
      /** @type {Map<string,{fileName:string,sha256:string,kind:string}[]>} */ const deliveredAssets = new Map();
      for (const item of Object.values(bundle)) {
        if (item.type === 'asset') {
          const digest = sha256(typeof item.source === 'string' ? Buffer.from(item.source) : item.source);
          for (const file of item.originalFileNames || []) {
            const owner = npmOwner(isAbsolute(file) ? file : join(config?.root || repo, file)); if (!owner) continue;
            const record = observed.get(owner.manifest) || {owner,inputs:new Map(),rendered:new Set()};
            const inputHash = sha256(readFileSync(owner.file));
            record.inputs.set(relative(owner.directory,owner.file).replaceAll('\\','/'),inputHash);
            observed.set(owner.manifest,record);
            if (digest === inputHash) {
              const assets = deliveredAssets.get(owner.manifest) || [];
              assets.push({fileName:item.fileName,sha256:digest,kind:extname(owner.file).slice(1)});
              deliveredAssets.set(owner.manifest,assets);
            }
          }
          continue;
        }
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
      if (cssObserverExpected && !cssObserverAttached) gaps.push({ecosystem:'npm',reason:'css-compiler-dependency-observer-unavailable'});
      for(const {owner,inputs,rendered} of observed.values()) {
        const proof=[{kind:'build-input',detail:'Observed by this production client transform/module graph; not all inputs survive tree shaking.'}];
        if(rendered.size)proof.push({kind:'bundled',detail:`Nonzero rendered JavaScript in ${[...rendered].sort().join(', ')}`});
        const assets = deliveredAssets.get(owner.manifest) || [];
        if (assets.length) proof.push({kind:'bundled',detail:`Actual emitted assets byte-identical to package inputs: ${assets.map(asset=>`${asset.fileName} sha256=${asset.sha256}`).sort().join(', ')}`});
        if ([...inputs.keys()].some(file=>file.endsWith('.css'))) proof.push({kind:'build-input',detail:'CSS files registered by the actual build; transformed CSS retention is not inferred from input presence.'});
        const {entry,materials}=packageEntry(owner,proof);
        if (assets.length) entry.emittedAssets = assets.sort((a,b)=>a.fileName.localeCompare(b.fileName,'en'));
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
      let inventory={schemaVersion:1,target:{platform:'frontend',triple:'browser',profile:'production',features:resolveProduct().features},
        provenance:{kind:'vite-client-build',inputs:[{label:'pnpm-lock.yaml',sha256:sha256(readFileSync(join(repo,'pnpm-lock.yaml')))},{label:'vite.config.js',sha256:sha256(Buffer.from(cfg))}]},entries,texts,gaps};
      const cachePath = join(repo,'resources/licenses/frontend-package-materials.json');
      const cacheBytes = readFileSync(cachePath);
      inventory = applyPackageMaterialCache(inventory,JSON.parse(cacheBytes.toString('utf8')));
      inventory.provenance.inputs.push({label:'resources/licenses/frontend-package-materials.json',sha256:sha256(cacheBytes)});
      this.emitFile({type:'asset',fileName:'license-inventory.frontend.json',source:serializeInventory(inventory)});
    }
  };
}
