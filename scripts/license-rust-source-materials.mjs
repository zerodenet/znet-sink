// Explicit maintenance collector. Normal license builds never fetch sources.
// Registry archives and core/Cargo Git checkouts are only read, never extracted
// into or changed in place. Cached materials retain exact package/revision ties.
import {readFileSync,existsSync,writeFileSync,mkdirSync,statSync} from 'node:fs';
import {dirname,join,resolve,basename} from 'node:path';
import {execFileSync,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {fileURLToPath} from 'node:url';
import {sha256,safeSource,argumentsOf} from './license-common.mjs';
const run=promisify(execFile);
const root=fileURLToPath(new URL('../',import.meta.url));
const legalNames=['LICENSE','LICENSE-MIT','LICENSE-APACHE','LICENSE-BSD','LICENSE_MIT','LICENSE_APACHE-2.0','LICENSE_APACHE','LICENSE.md','LICENSE.txt','COPYING','NOTICE','COPYRIGHT'];
const maximum=2*1024*1024;
export function packageIdentity(p){
  return {ecosystem:'cargo',name:p.name,version:p.version,source:p.source?safeSource(p.source):`workspace:${p.name}`};
}
export function registryArchive(p) {
  if(!p.source?.startsWith('registry+'))return null;
  const dir=dirname(p.manifest_path),index=basename(dirname(dir));
  const archive=join(dirname(dirname(dirname(dir))),'cache',index,`${p.name}-${p.version}.crate`);
  return existsSync(archive)?archive:null;
}
export function registryArchiveSha256(p) {
  const archive=registryArchive(p);if(!archive)return null;
  if(statSync(archive).size>128*1024*1024)throw new Error('Cargo registry archive exceeds audit budget');
  const checksumPath=join(dirname(p.manifest_path),'.cargo-checksum.json');
  const hash=sha256(readFileSync(archive));
  if(existsSync(checksumPath)){const checksum=JSON.parse(readFileSync(checksumPath,'utf8'));if(!/^[a-f0-9]{64}$/.test(checksum.package||'')||hash!==checksum.package)throw new Error('Cargo registry archive/checksum mismatch');}
  return hash;
}
function lockedChecksum(p,lock) {
  const fields=block=>Object.fromEntries([...block.matchAll(/^(name|version|source|checksum) = "([^"\n]+)"$/gm)].map(m=>[m[1],m[2]]));
  const matches=lock.split(/^\[\[package\]\]$/m).slice(1).map(fields).filter(row=>row.name===p.name&&row.version===p.version&&row.source===p.source);
  if(matches.length!==1||!/^[a-f0-9]{64}$/.test(matches[0].checksum||''))throw new Error('Exact locked Cargo package checksum unavailable');
  return matches[0].checksum;
}
function archiveBytes(p,member) {
  if(!/^[A-Za-z0-9_.+-]+$/.test(p.name)||!/^[A-Za-z0-9_.+-]+$/.test(p.version)||member.startsWith('/')||member.split('/').includes('..'))throw new Error('Unsafe Cargo archive member');
  return execFileSync('tar',['-xOzf',registryArchive(p),`${p.name}-${p.version}/${member}`],{maxBuffer:maximum,stdio:['ignore','pipe','ignore']});
}
export function reviewedRegistryBinding(p,lock) {
  const archiveHash=registryArchiveSha256(p);
  if(!archiveHash||archiveHash!==lockedChecksum(p,lock))throw new Error('Cargo.lock registry archive checksum mismatch');
  const manifest=readFileSync(p.manifest_path);
  if(!manifest.equals(archiveBytes(p,'Cargo.toml')))throw new Error('Installed Cargo manifest differs from locked archive');
  let vcs=null;try{vcs=JSON.parse(archiveBytes(p,'.cargo_vcs_info.json').toString('utf8'));}catch{}
  if(vcs?.git?.dirty===true)throw new Error('Dirty archived Cargo VCS source requires separate review');
  if(vcs?.git?.sha1&&!/^[a-f0-9]{40}$/.test(vcs.git.sha1))throw new Error('Invalid archived Cargo VCS revision');
  return {identity:packageIdentity(p),expected:{manifestSha256:sha256(manifest),cargoPackageIdSha256:sha256(p.id),registryArchiveSha256:archiveHash},vcs};
}
function textOf(bytes) {
  if(bytes.length>maximum||bytes.includes(0))throw new Error('Legal source is binary or exceeds budget');
  const text=new TextDecoder('utf-8',{fatal:true,ignoreBOM:true}).decode(bytes);
  if(!text.trim())throw new Error('Empty legal source');
  return text;
}
export function archiveMaterial(p,binding,member,kind) {
  const bytes=archiveBytes(p,member),text=textOf(bytes);
  const url=`https://static.crates.io/crates/${p.name}/${p.name}-${p.version}.crate`;
  return {name:basename(member),kind,source:`${url}#${member}`,sha256:sha256(bytes),text,
    origin:{url,sha256:binding.expected.registryArchiveSha256,member:`${p.name}-${p.version}/${member}`}};
}
function repository(p) {
  const url=new URL(p.repository||'');const parts=url.pathname.replace(/\.git\/?$/,'').split('/').filter(Boolean);
  if(url.protocol!=='https:'||url.hostname!=='github.com'||url.username||url.password||parts.length!==2||parts.some(p=>!/^[A-Za-z0-9_.-]+$/.test(p)))throw new Error('Pinned GitHub repository unavailable');
  return `https://raw.githubusercontent.com/${parts.join('/')}`;
}
async function rawMaterial(base,revision,name) {
  const url=`${base}/${revision}/${name}`;
  try {
    const {stdout}=await run('curl',['--proto','=https','--fail','--location','--silent','--show-error','--max-time','25',url],{encoding:'buffer',maxBuffer:maximum});
    const text=textOf(stdout);
    return {name,kind:/^(NOTICE|COPYRIGHT)/.test(name)?'notice':'license',source:url,sha256:sha256(stdout),text,
      origin:{url,sha256:sha256(stdout),revision,member:name}};
  }catch(error){if(error.code===22&&/404/.test(String(error.stderr)))return null;throw new Error(`Pinned legal material acquisition failed (${revision}/${name})`);}
}
// Reviewed exceptions retain publication evidence instead of treating a tag as
// an archived VCS claim or treating common license terms as component attribution.
export function selectorsMaterials(p,binding) {
  const all=archiveBytes(p,'lib.rs'),end=all.indexOf(Buffer.from('*/'))+2;
  if(end!==205)throw new Error('Reviewed selectors notice changed');
  const notice=all.subarray(0,end),noticeHash=sha256(notice);
  if(noticeHash!=='7523df2468fda84e4df377ee0cfd844a83742f39e0425e7c7819a82e3e25f840')throw new Error('Reviewed selectors notice mismatch');
  const canonical='https://mozilla.org/MPL/2.0/';
  const bytes=readFileSync(join(root,'LICENSE'));
  if(sha256(bytes)!=='3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04')throw new Error('Reviewed common MPL terms changed');
  const revision='bfaf063de33e59e77c19a9c447f28f023e5f4b5f',url=`https://raw.githubusercontent.com/zerodenet/znet-sink/${revision}/LICENSE`;
  const archiveUrl=`https://static.crates.io/crates/selectors/selectors-${p.version}.crate`;
  return {identity:binding.identity,expected:binding.expected,resolves:[],unresolved:['package-specific-legal-material-not-provided'],materials:[
    {name:'selectors source MPL notice',kind:'notice',source:`${archiveUrl}#lib.rs:bytes=0-205`,sha256:noticeHash,text:textOf(notice),origin:{url:archiveUrl,sha256:binding.expected.registryArchiveSha256,member:`selectors-${p.version}/lib.rs`,byteRange:{start:0,end:205}}},
    {name:'MPL 2.0 common terms (not component attribution)',kind:'license',source:url,sha256:sha256(bytes),text:textOf(bytes),origin:{url,sha256:sha256(bytes),revision,member:'LICENSE'}}],
    acquisition:{kind:'source-declared-canonical-common-terms',canonicalUrl:canonical,officialDownload:'Proxy tunnel HTTP 403; no official download body obtained.',actualBody:'Existing standard MPL 2.0 terms from the pinned client repository LICENSE; contains no client or selectors component copyright. Not a selectors repository file.',vcsRevision:binding.vcs.git.sha1,packagePath:binding.vcs.path_in_vcs}};
}
export function taoTaggedMaterials(p,binding,archive) {
  const revision='340684d932b6126bed449fc1f4edca81776ad592',prefix=`tao-${revision}`;
  const url=`https://codeload.github.com/tauri-apps/tao/tar.gz/${revision}`;
  const read=member=>execFileSync('tar',['-xOzf','-',`${prefix}/${member}`],{input:archive,maxBuffer:maximum,stdio:['pipe','pipe','ignore']});
  const compared=[];
  // Complete package source/example set plus all packaged repository metadata;
  // Cargo.toml is normalized by Cargo; Cargo.lock is publication-generated.
  for(const [member,repositoryMember] of [['Cargo.toml.orig','tao-macros/Cargo.toml'],['README.md','README.md'],['CHANGELOG.md','tao-macros/CHANGELOG.md'],['src/lib.rs','tao-macros/src/lib.rs'],['examples/generate_package_name.rs','tao-macros/examples/generate_package_name.rs'],['examples/android_fn.rs','tao-macros/examples/android_fn.rs']]) {
    const published=archiveBytes(p,member),tagged=read(repositoryMember);
    if(!published.equals(tagged))throw new Error('Exact tao-macros release source comparison failed');
    compared.push({publishedMember:member,repositoryMember,sha256:sha256(published)});
  }
  const bytes=read('LICENSE');
  if(!textOf(bytes).includes('Version 2.0, January 2004'))throw new Error('Reviewed tao Apache terms unavailable');
  return {identity:binding.identity,expected:binding.expected,resolves:[],unresolved:['alternative-MIT-license-text-not-provided'],materials:[{name:'LICENSE (Apache 2.0 only)',kind:'license',source:`https://raw.githubusercontent.com/tauri-apps/tao/${revision}/LICENSE`,sha256:sha256(bytes),text:textOf(bytes),origin:{url,sha256:sha256(archive),revision,member:`${prefix}/LICENSE`}}],acquisition:{kind:'exact-release-tag-and-published-source-comparison',tag:'tao-macros-v0.1.3',revision,archiveSha256:sha256(archive),compared,excludedGenerated:['Cargo.toml','Cargo.lock'],limitations:'Published archive has no VCS record. Exact version-specific tag plus all published source/examples and original manifest match is recorded as separate evidence. Only the actual Apache alternative is available; no MIT text or attribution was invented.'}};
}
export async function collectRegistryMaterials(packages,lock,{fetchMaterials}={}) {
  const rows=[],gaps=[],groups=new Map();
  for(const p of [...packages].sort((a,b)=>a.id.localeCompare(b.id,'en'))) {
    if(!p.source?.startsWith('registry+')){gaps.push({name:p.name,version:p.version,reason:'non-registry-package-requires-own-source-review'});continue;}
    const binding=reviewedRegistryBinding(p,lock);
    // Reviewed actual file: the archive's COPYRIGHT-RUST contains the complete
    // project terms as well as notices. Never borrow another crate's template.
    if(p.name==='cesu8'&&p.version==='1.1.0') {
      rows.push({identity:binding.identity,expected:binding.expected,materials:[archiveMaterial(p,binding,'COPYRIGHT-RUST.txt','license')],resolves:[]});continue;
    }
    if(p.name==='selectors'&&p.version==='0.36.1'){rows.push(selectorsMaterials(p,binding));continue;}
    if(p.name==='tao-macros'&&p.version==='0.1.3'){
      const {stdout}=await run('curl',['--proto','=https','--fail','--location','--silent','--show-error','--max-time','25','https://codeload.github.com/tauri-apps/tao/tar.gz/340684d932b6126bed449fc1f4edca81776ad592'],{encoding:'buffer',maxBuffer:16*1024*1024});
      rows.push(taoTaggedMaterials(p,binding,stdout));continue;
    }
    const revision=binding.vcs?.git?.sha1;
    if(!revision){gaps.push({identity:binding.identity,expected:binding.expected,reason:'published-archive-has-no-pinned-vcs-revision',audit:'Published .crate has no immutable repository revision. No current/tag license was substituted.'});continue;}
    const base=repository(p),key=`${base}@${revision}`;
    if(!groups.has(key))groups.set(key,fetchMaterials?fetchMaterials(base,revision):
      (async()=>{const found=[];for(let index=0;index<legalNames.length;index+=3){const batch=await Promise.all(legalNames.slice(index,index+3).map(name=>rawMaterial(base,revision,name)));found.push(...batch.filter(Boolean));}return found;})());
    const materials=await groups.get(key);
    if(!materials.some(m=>m.kind==='license')){gaps.push({identity:binding.identity,expected:binding.expected,revision,packagePath:binding.vcs.path_in_vcs??null,reason:'pinned-repository-legal-terms-not-provided',audit:{searchedRepositoryRootNames:legalNames}});continue;}
    // VCS claims remain bound to the actual checked registry archive. A clean
    // Cargo VCS identity does not claim these files were in the published .crate.
    rows.push({identity:binding.identity,expected:binding.expected,materials,resolves:[],
      acquisition:{kind:'published-crate-pinned-repository',registryArchiveSha256:binding.expected.registryArchiveSha256,
        vcsRevision:revision,packagePath:binding.vcs.path_in_vcs??null}});
  }
  return {schemaVersion:1,packages:rows,gaps};
}
if(process.argv[1]===fileURLToPath(import.meta.url)) {
  const args=argumentsOf(process.argv.slice(2));
  if(!args.metadata||!args.inventory||!args['write-cache'])throw new Error('--metadata, --inventory and explicit --write-cache are required');
  const metadata=args.metadata.split(',').map(path=>JSON.parse(readFileSync(path,'utf8')));
  const wanted=new Set(args.inventory.split(',').flatMap(path=>JSON.parse(readFileSync(path,'utf8')).entries.filter(row=>row.ecosystem==='cargo'&&row.missing.includes('license-text-not-provided-by-installed-material')).map(row=>`${row.name}@${row.version}:${row.source}`)));
  const packages=new Map();for(const meta of metadata)for(const p of meta.packages)if(wanted.has(`${p.name}@${p.version}:${p.source?safeSource(p.source):`workspace:${p.manifest_path.replace(meta.workspace_root+'/','').replace(/\/Cargo.toml$/,'')}`}`))packages.set(p.id,p);
  const result=await collectRegistryMaterials([...packages.values()],readFileSync(args.lock||join(root,'src-tauri/Cargo.lock'),'utf8'));
  mkdirSync(dirname(resolve(args['write-cache'])),{recursive:true});writeFileSync(args['write-cache'],JSON.stringify(result,null,2)+'\n');
  console.log(`Pinned Rust package materials: ${result.packages.length} packages, ${result.gaps.length} acquisition gaps`);
}
