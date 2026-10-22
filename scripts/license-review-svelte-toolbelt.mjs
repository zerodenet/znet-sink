// Offline, explicit maintenance of the one reviewed npm package. Downloads are
// supplied by the maintainer; normal builds never access the registry here.
import {readFileSync,writeFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {sha256,argumentsOf} from './license-common.mjs';
import {lockedNpmIntegrity} from './license-vite.mjs';
import {svelteToolbeltReview as reviewed,validateReviewedLicenseMapping} from './license-reviewed-mapping.mjs';
/** @param {{archive:Buffer,manifest:Buffer,installedLicense:Buffer,lock:string,metadata:Buffer}} input */
export function reviewedSvelteToolbeltMaterial(input) {
  if(input.archive.length>2*1024*1024||sha256(input.archive)!==reviewed.archiveSha256)throw new Error('Reviewed npm archive mismatch');
  const integrity='sha512-'+createHash('sha512').update(input.archive).digest('base64');
  if(integrity!==reviewed.expected.lockIntegrity||lockedNpmIntegrity(input.lock,'svelte-toolbelt','0.10.6')!==integrity)throw new Error('Exact npm lock integrity mismatch');
  const read=/** @param {string} name */ name=>execFileSync('tar',['-xOzf','-',`package/${name}`],{input:input.archive,maxBuffer:2*1024*1024,stdio:['pipe','pipe','ignore']});
  const manifest=read('package.json'),license=read('LICENSE');
  if(!manifest.equals(input.manifest)||sha256(manifest)!==reviewed.expected.manifestSha256||!license.equals(input.installedLicense)||sha256(license)!==reviewed.materialSha256)throw new Error('Installed npm source differs from reviewed archive');
  const declared=JSON.parse(manifest.toString('utf8')),metadata=JSON.parse(input.metadata.toString('utf8'));
  const url='https://registry.npmjs.org/svelte-toolbelt/-/svelte-toolbelt-0.10.6.tgz';
  if(declared.name!=='svelte-toolbelt'||declared.version!=='0.10.6'||declared.license!==undefined||declared.licenses!==undefined||metadata.name!==declared.name||metadata.version!==declared.version||metadata.license!==undefined||metadata.dist?.integrity!==integrity||metadata.dist?.tarball!==url)throw new Error('Reviewed npm publication metadata mismatch');
  const text=new TextDecoder('utf-8',{fatal:true}).decode(license);
  const pkg={identity:{...reviewed.identity},expected:{...reviewed.expected},resolves:[],materials:[{name:'LICENSE (published MIT text)',kind:'license',source:`${url}#package/LICENSE`,sha256:reviewed.materialSha256,text,origin:{url,sha256:reviewed.archiveSha256,member:'package/LICENSE'}}],
    reviewedLicenseMapping:{expression:'MIT',basis:'reviewed-package-license-text',manifestDeclared:false,materialSha256:reviewed.materialSha256},
    acquisition:{kind:'locked-npm-archive-and-installed-byte-comparison',archiveSha256:reviewed.archiveSha256,lockIntegrity:integrity,metadata:{url:'https://registry.npmjs.org/svelte-toolbelt/0.10.6',sha256:sha256(input.metadata)},compared:['package/package.json','package/LICENSE'],limitations:'The exact published LICENSE explicitly says MIT License and provides its own copyright notices. This is a reviewed SPDX mapping of that text, not a package.json declaration. The manifest declaration gap and final-link retention limits remain.'}};
  validateReviewedLicenseMapping(pkg);
  return pkg;
}
if(process.argv[1]===fileURLToPath(import.meta.url)) {
  const args=argumentsOf(process.argv.slice(2));
  for(const key of ['archive','installed','metadata','lock','write-cache'])if(!args[key])throw new Error(`--${key} is required`);
  const pkg=reviewedSvelteToolbeltMaterial({archive:readFileSync(args.archive),manifest:readFileSync(`${args.installed}/package.json`),installedLicense:readFileSync(`${args.installed}/LICENSE`),metadata:readFileSync(args.metadata),lock:readFileSync(args.lock,'utf8')});
  const cache=JSON.parse(readFileSync(args['write-cache'],'utf8'));
  if(cache.schemaVersion!==1||!Array.isArray(cache.packages))throw new Error('Invalid package cache');
  cache.packages=cache.packages.filter(/** @param {any} p */ p=>JSON.stringify(p.identity)!==JSON.stringify(pkg.identity));
  cache.packages.push(pkg);cache.packages.sort(/** @param {any} a @param {any} b */(a,b)=>JSON.stringify(a.identity).localeCompare(JSON.stringify(b.identity),'en'));
  writeFileSync(args['write-cache'],JSON.stringify(cache,null,2)+'\n');
  console.log('Recorded exact svelte-toolbelt 0.10.6 MIT text mapping; manifest declaration remains unavailable.');
}
