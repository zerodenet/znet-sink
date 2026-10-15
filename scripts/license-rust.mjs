import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sha256,entryId,safeSource,readTextMaterials,attachMaterials,serializeInventory,argumentsOf } from './license-common.mjs';
import { applyPackageMaterialCache } from './license-materials.mjs';
import { registryArchiveSha256 } from './license-rust-source-materials.mjs';
const root=fileURLToPath(new URL('../',import.meta.url));

export function rustPackageInventory(metadata,{compilerMessages=[]}={}) {
  const packages=new Map(metadata.packages.map(p=>[p.id,p]));
  const nodes=new Map(metadata.resolve.nodes.map(n=>[n.id,n]));
  const gui=metadata.packages.find(p=>p.name==='gui' && metadata.workspace_members.includes(p.id));
  if(!gui)throw new Error('GUI root package not resolved');
  const selected=new Map(),queue=[{id:gui.id,host:false}];
  while(queue.length) {
    const {id,host}=queue.shift();const p=packages.get(id);if(!p)throw new Error('Missing Cargo package');
    const macro=p.targets.some(t=>t.kind.includes('proc-macro'));
    const kind=host||macro?'build-tool':'resolved-target';
    const set=selected.get(id)||new Set();if(set.has(kind))continue;set.add(kind);selected.set(id,set);
    for(const dep of nodes.get(id)?.deps||[]) {
      const eligible=dep.dep_kinds.filter(k=>k.kind!=='dev');
      if(eligible.length)queue.push({id:dep.pkg,host:host||macro||eligible.every(k=>k.kind==='build')});
    }
  }
  const messages=new Map();
  for(const message of compilerMessages) if(message.reason==='compiler-artifact') {
    const files=message.filenames||[];
    const codegen=files.some(path=>/\.(?:rlib|so|a|dll|dylib)$/.test(path));
    if(codegen && files.every(path=>existsSync(path)))messages.set(message.package_id,message);
  }
  const entries=[],texts=[],gaps=[];
  for(const [id,kinds] of selected) {
    const p=packages.get(id),directory=dirname(p.manifest_path);
    const source=p.source?safeSource(p.source):`workspace:${relative(metadata.workspace_root,directory).replaceAll('\\','/')||'.'}`;
    const evidence=[...kinds].sort().map(kind=>({kind,detail:kind==='build-tool'?'Cargo target resolve: build dependency/proc macro; not runtime code delivery proof.':'Cargo target-filtered normal dependency graph; resolved does not prove code generation/link retention.'}));
    const observed=messages.get(id);
    if(observed)evidence.push({kind:'compiled',detail:'Observed compiler-artifact with existing code-generated output; dead-code/link retention is not inferred.',targetKinds:observed.target.kind,features:observed.features});
    const memberManifest=readFileSync(p.manifest_path,'utf8');
    const materials=readTextMaterials(directory,`cargo:${p.name}@${p.version}`,p.license_file?resolve(directory,p.license_file):null);
    // Workspace-inherited licensing can live above a crate. Use actual enclosing workspace
    // material, and record its provenance; never synthesize the declared SPDX identifier.
    if(!materials.texts.some(t=>t.kind==='license')) {
      let cursor=dirname(directory);
      for(let depth=0;depth<5 && cursor!==dirname(cursor);depth++,cursor=dirname(cursor)) {
        const manifest=join(cursor,'Cargo.toml');
        if(!existsSync(manifest))continue;
        const content=readFileSync(manifest,'utf8');
        if(!/^\[workspace(?:\.|\])/m.test(content))continue;
        const inherits=/license\.workspace\s*=\s*true|license\s*=\s*\{[^}]*workspace\s*=\s*true/.test(memberManifest);
        if(!inherits)break;
        const enclosing=readTextMaterials(cursor,`cargo-workspace:${p.source?safeSource(p.source):'local'}`,null,0);
        // Enclosing scan is restricted to top-level legal material below.
        materials.texts.push(...enclosing.texts);
        materials.gaps.push(...enclosing.gaps);break;
      }
    }
    if(!p.source && p.license==='MPL-2.0' && directory.startsWith(root) && !materials.texts.some(t=>t.kind==='license'))materials.texts.push(...readTextMaterials(root,'project:znet-sink',null,0).texts);
    const entry=attachMaterials({id:entryId('cargo',p.name,p.version,source),ecosystem:'cargo',name:p.name,version:p.version,source,declaredLicense:p.license||null,
      cargoPackageIdSha256:sha256(id),...(p.source?.startsWith('registry+')?{registryArchiveSha256:registryArchiveSha256(p)}:{}),manifestSha256:sha256(readFileSync(p.manifest_path)),resolvedFeatures:nodes.get(id)?.features||[],evidence},materials);
    entries.push(entry);texts.push(...materials.texts);
    for(const reason of entry.missing)gaps.push({ecosystem:'cargo',id:entry.id,reason});
  }
  return {entries,texts,gaps};
}

export function applyRustMaterialCache(inventory,cache) {
  const result=applyPackageMaterialCache(inventory,cache);
  for(const pkg of cache.packages) {
    for(const reason of pkg.unresolved||[]) {
      if(!['package-specific-legal-material-not-provided','alternative-MIT-license-text-not-provided'].includes(reason))throw new Error('Unknown Rust material limitation');
      for(const row of result.entries)if(['ecosystem','name','version','source'].every(key=>row[key]===pkg.identity[key])&&!row.missing.includes(reason)) {
        row.missing.push(reason);result.gaps.push({ecosystem:row.ecosystem,id:row.id,reason});
      }
    }
  }
  return result;
}

export function localLicenseInputs(metadata,entries) {
  const selected=new Set(entries.filter(e=>e.ecosystem==='cargo').map(e=>e.cargoPackageIdSha256));
  const inputs=new Map();
  for(const p of metadata.packages) {
    if(p.source||!selected.has(sha256(p.id)))continue;
    for(const file of [p.manifest_path,...(p.license_file?[resolve(dirname(p.manifest_path),p.license_file)]:[])]) {
      const label=relative(root,file).replaceAll('\\','/');
      if(label.startsWith('../')||label.startsWith('/'))continue;
      inputs.set(label,{label,sha256:sha256(readFileSync(file))});
    }
  }
  return [...inputs.values()].sort((a,b)=>a.label.localeCompare(b.label,'en'));
}

export function rustInventory(metadata,{target,profile,features,compilerMessages=[]}) {
  const {entries,texts,gaps}=rustPackageInventory(metadata,{compilerMessages});
  // Standard library arrives from the selected toolchain, outside Cargo.lock.
  // Its shipped copyright inventory covers library source, not final-link retention.
  const rustc=process.env.RUSTC||'rustc';
  const sysroot=execFileSync(rustc,['--print','sysroot'],{encoding:'utf8'}).trim();
  const toolchain=execFileSync(rustc,['-vV'],{encoding:'utf8'}).trim();
  const library=join(sysroot,'lib/rustlib',target,'lib');
  if(existsSync(library)) {
    const legal=join(sysroot,'share/doc/rust/licenses');
    const licenses=readTextMaterials(legal,'rust-toolchain-license-terms',null,0);
    for(const name of ['Apache-2.0.txt','MIT.txt','LLVM-exception.txt'])if(existsSync(join(legal,name)))licenses.texts.push(...readTextMaterials(legal,'rust-toolchain-license-terms',join(legal,name),0).texts);
    const copyright=join(sysroot,'share/doc/rust/COPYRIGHT-library.html');
    if(existsSync(copyright))licenses.texts.push(...readTextMaterials(join(sysroot,'share/doc/rust'),'rust-toolchain-library',copyright,0).texts.filter(t=>t.name==='COPYRIGHT-library.html'));
    else licenses.gaps.push({reason:'rust-toolchain-library-copyright-not-provided'});
    const entry=attachMaterials({id:entryId('platform','Rust standard library',toolchain.split('\n')[0],target),ecosystem:'platform',name:'Rust standard library',version:toolchain.split('\n')[0],declaredLicense:null,source:'rust-toolchain:selected-target',evidence:[{kind:'build-input',detail:'Selected target standard library exists in the active Rust sysroot. Shipped source-level legal inventory does not prove each library component survives final linking.'}],toolchain},licenses);
    // The toolchain inventory states its own component declarations; no aggregate
    // SPDX expression is invented for its many library contributors.
    entry.missing=entry.missing.filter(reason=>reason!=='license-declaration-not-provided');
    entries.push(entry);texts.push(...licenses.texts);for(const reason of entry.missing)gaps.push({ecosystem:'platform',id:entry.id,reason});
  }else gaps.push({ecosystem:'platform',reason:'target-standard-library-not-installed'});
  return {schemaVersion:1,target:{platform:target.includes('android')?'android':target.includes('ios')?'ios':'desktop',triple:target,profile,features:[...features].sort()},
    provenance:{kind:compilerMessages.length?'cargo-target-resolve-with-compiler-receipts':'cargo-target-resolve',
      inputs:[...localLicenseInputs(metadata,entries).filter(input=>input.label!=='src-tauri/Cargo.toml'),{label:'src-tauri/Cargo.toml',sha256:sha256(readFileSync(join(root,'src-tauri/Cargo.toml')))},{label:'src-tauri/Cargo.lock',sha256:sha256(readFileSync(join(root,'src-tauri/Cargo.lock')))}]},entries,texts,gaps};
}

if(process.argv[1]===fileURLToPath(import.meta.url)) {
  const args=argumentsOf(process.argv.slice(2));
  if(!args.target||!args.output)throw new Error('--target and --output are required');
  const features=(args.features||'').split(',').filter(Boolean).sort();
  const params=['metadata','--manifest-path',join(root,'src-tauri/Cargo.toml'),'--format-version','1','--offline','--locked','--filter-platform',args.target,'--no-default-features'];
  if(features.length)params.push('--features',features.join(','));
  const metadata=JSON.parse(execFileSync(process.env.CARGO||'cargo',params,{encoding:'utf8',maxBuffer:32*1024*1024}));
  const compilerMessages=args['cargo-messages']?readFileSync(args['cargo-messages'],'utf8').split('\n').flatMap(line=>{try{return [JSON.parse(line)];}catch{return [];}}):[];
  let result=rustInventory(metadata,{target:args.target,profile:args.profile||'release',features,compilerMessages});
  const cachePath=join(root,'resources/licenses/rust-package-materials.json');
  const cacheBytes=readFileSync(cachePath);
  result=applyRustMaterialCache(result,JSON.parse(cacheBytes.toString('utf8')));
  result.provenance.inputs.push({label:'resources/licenses/rust-package-materials.json',sha256:sha256(cacheBytes)});
  mkdirSync(dirname(resolve(args.output)),{recursive:true});writeFileSync(args.output,serializeInventory(result));
  console.log(`Cargo license graph ${args.target}: ${result.entries.length} dependencies, ${result.gaps.length} explicit gaps`);
}
