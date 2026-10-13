// Exercise the real Rust build hook without rebuilding the GUI or invoking Node from it.
import {readFileSync,writeFileSync,mkdirSync,mkdtempSync,readdirSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {tmpdir} from 'node:os';
import {execFileSync,spawnSync} from 'node:child_process';
import {sha256} from './license-common.mjs';
const root=fileURLToPath(new URL('../',import.meta.url)),work=mkdtempSync(join(tmpdir(),'znet-license-rust-hook-'));
try{
 const deps=join(root,'src-tauri/target/debug/deps');
 const library=name=>{const candidates=readdirSync(deps).filter(n=>n.startsWith(`lib${name}-`)&&n.endsWith('.rlib')).sort();if(!candidates.length)throw new Error(`Build workspace first: ${name} rlib missing`);return join(deps,candidates[0]);};
 const harness=join(work,'hook.rs'),binary=join(work,process.platform==='win32'?'hook.exe':'hook');
 writeFileSync(harness,`#[path=${JSON.stringify(join(root,'src-tauri/build_license_inventory.rs'))}] mod hook;fn main(){hook::emit(&["tool-dns"]);}`);
 execFileSync('rustc',[harness,'--edition=2021','-L',`dependency=${deps}`,'--extern',`sha2=${library('sha2')}`,'--extern',`serde_json=${library('serde_json')}`,'-o',binary]);
 const out=join(work,'out');mkdirSync(out);
 const text='Actual license terms',hash=sha256(text),prepared={schemaVersion:1,target:{triple:'aarch64-linux-android',profile:'release',features:['tool-dns']},provenance:{inputs:['Cargo.toml','Cargo.lock'].map(name=>({label:`src-tauri/${name}`,sha256:sha256(readFileSync(join(root,'src-tauri',name)))}))},entries:[],texts:[{id:`sha256-${hash}`,sha256:hash,text}],gaps:[]};
 const input=join(work,'inventory.json');
 const run=(value,extra={})=>{if(value)writeFileSync(input,JSON.stringify(value));return spawnSync(binary,[],{cwd:join(root,'src-tauri'),env:{...process.env,TARGET:'aarch64-linux-android',PROFILE:'release',OUT_DIR:out,ZNET_LICENSE_INVENTORY:input,ZNET_REQUIRE_LICENSE_INVENTORY:'1',...extra},encoding:'utf8'});};
 if(run(prepared).status!==0)throw new Error('Valid target inventory rejected');
 const reject=value=>{if(run(value).status===0)throw new Error('Invalid release inventory accepted');};
 reject({...prepared,target:{...prepared.target,triple:'aarch64-apple-ios'}});
 reject({...prepared,target:{...prepared.target,features:[]}});
 reject({...prepared,provenance:{inputs:prepared.provenance.inputs.map(i=>({...i,sha256:'0'.repeat(64)}))}});
 reject({...prepared,texts:[{...prepared.texts[0],text:'tampered'}]});
 reject({...prepared,entries:[{id:'row',licenseTextIds:['absent'],noticeTextIds:[]}]});
 const debug=run(prepared,{PROFILE:'debug'});if(debug.status!==0)throw new Error('Debug initialization must remain available');
 const stub=JSON.parse(readFileSync(join(out,'license-inventory.rust.json'),'utf8'));if(stub.provenance.kind!=='inventory-unavailable'||stub.entries.length!==0)throw new Error('Debug mismatch falsely claims inventory available');
 console.log('Rust license build hook: matching release, target/features/input SHA/text tamper rejection, honest debug stub passed');
}finally{rmSync(work,{recursive:true,force:true});}
