import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, symlinkSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { npmOwner, packageEntry, lockedNpmIntegrity, licenseInventory } from './license-vite.mjs';
import { applyPackageMaterialCache } from './license-materials.mjs';
import { sha256, serializeInventory } from './license-common.mjs';
const repo = fileURLToPath(new URL('../', import.meta.url));
const cache = JSON.parse(readFileSync(join(repo, 'resources/licenses/frontend-package-materials.json'), 'utf8'));
const lock = readFileSync(join(repo, 'pnpm-lock.yaml'), 'utf8');

function installedEntry(pkg) {
  const manifest = realpathSync(join(repo, 'node_modules', pkg.identity.name, 'package.json'));
  const owner = {manifest, directory:dirname(manifest), value:JSON.parse(readFileSync(manifest, 'utf8'))};
  return packageEntry(owner, [{kind:'build-input', detail:'Test reads the actual locked installed package, not a distribution claim.'}], lock);
}
function fixtureInventory(entry, texts) {
  return {schemaVersion:1, entries:[entry], texts, gaps:entry.missing.map(reason=>({ecosystem:'npm',id:entry.id,reason}))};
}

test('npm lock evidence uses an exact packages resolution, never dependency ranges or snapshot names', () => {
  for (const pkg of cache.packages) assert.equal(lockedNpmIntegrity(lock, pkg.identity.name, pkg.identity.version), pkg.expected.lockIntegrity);
  const example = "lockfileVersion: '9.0'\npackages:\n  '@fixture/css@1.2.3':\n    resolution: {integrity: sha512-YWJjZA==}\nsnapshots:\n  '@fixture/css@1.2.3(peer@1.0.0)': {}\n";
  assert.equal(lockedNpmIntegrity(example,'@fixture/css','1.2.3'),'sha512-YWJjZA==');
  assert.equal(lockedNpmIntegrity(example,'@fixture/css','^1.2.3'),null);
  assert.equal(lockedNpmIntegrity(example,'@fixture/css','1.2.30'),null);
  assert.equal(lockedNpmIntegrity(example.replace('9.0','8.0'),'@fixture/css','1.2.3'),null);
  assert.equal(lockedNpmIntegrity(example.replace('snapshots:',"  '@fixture/css@1.2.3':\n    resolution: {integrity: sha512-YWJjZA==}\nsnapshots:"),'@fixture/css','1.2.3'),null);
});

test('reviewed npm bodies bind actual package identity, manifest and archive integrity; notices retain raw attribution', () => {
  for (const pkg of cache.packages) {
    const {entry, materials} = installedEntry(pkg);
    assert.equal(entry.manifestSha256, pkg.expected.manifestSha256);
    assert.equal(entry.lockIntegrity, pkg.expected.lockIntegrity);
    assert.equal(entry.source, pkg.identity.source);
    const applied = applyPackageMaterialCache(fixtureInventory(entry,materials.texts),cache);
    const resolved = applied.entries[0];
    assert.ok(resolved.licenseTextIds.length);
    assert.ok(resolved.noticeTextIds.length);
    assert.ok(applied.texts.some(text=>text.text.includes('Copyright 2019-2023 Tauri Programme within The Commons Conservancy')));
    for (const material of pkg.materials) {
      assert.equal(sha256(Buffer.from(material.text)),material.sha256);
      assert.match(material.origin.url,/^https:\/\/(raw\.githubusercontent\.com|registry\.npmjs\.org)\//);
      assert.match(material.origin.sha256,/^[a-f0-9]{64}$/);
      if (material.kind === 'license') assert.ok(material.origin.url.includes(pkg.acquisition.gitHead));
      else {
        assert.equal(material.origin.sha256,pkg.acquisition.archiveSha256);
        assert.equal(material.origin.byteRange[1]-material.origin.byteRange[0],Buffer.byteLength(material.text));
      }
      assert.ok(resolved.materialReferences.some(reference=>reference.sha256===material.sha256 && reference.origin?.url===material.origin.url));
    }
    assert.equal(serializeInventory(applied),serializeInventory(applied));
  }
});

test('package input drift and body tampering fail; another version cannot borrow cached materials', () => {
  const pkg = cache.packages[0], {entry,materials} = installedEntry(pkg);
  for (const field of ['manifestSha256','lockIntegrity']) {
    const changed = {...entry,[field]:'changed'};
    assert.throws(()=>applyPackageMaterialCache(fixtureInventory(changed,materials.texts),cache),/Stale package material binding/);
  }
  const tampered = structuredClone(cache); tampered.packages[0].materials[0].text += 'changed';
  assert.throws(()=>applyPackageMaterialCache(fixtureInventory(entry,materials.texts),tampered),/digest/);
  const different = {...entry,version:entry.version+'-other'};
  const untouched = applyPackageMaterialCache(fixtureInventory(different,materials.texts),cache);
  assert.deepEqual(untouched.entries[0].licenseTextIds,entry.licenseTextIds);
  assert.ok(untouched.entries[0].missing.includes('license-text-not-provided-by-installed-material'));
});

test('actual Vite and Tailwind build records imported CSS/font assets and rendered JS, excluding installed unused packages', async () => {
  const root = mkdtempSync(join(tmpdir(),'znet-frontend-license-build-'));
  const write = (path,body) => {mkdirSync(dirname(path),{recursive:true});writeFileSync(path,body);};
  const pkg = (name,manifest,files) => {
    const directory = join(root,'node_modules',name);
    write(join(directory,'package.json'),JSON.stringify({name,version:'1.0.0',license:'MIT',repository:'https://example.invalid/test-fixture',...manifest}));
    write(join(directory,'LICENSE'),'Test fixture only: original test terms. Copyright test fixture author.');
    for (const [file,body] of Object.entries(files)) write(join(directory,file),body);
    return directory;
  };
  try {
    const css = pkg('@fixture/css',{exports:{'.':{style:'./index.css'}}},{'index.css':'@font-face { font-family: fixture; src: url("./actual.woff2") format("woff2"); } .fixture { color: red; }','actual.woff2':Buffer.from('synthetic test-only font asset; no production font claim')});
    pkg('@fixture/js',{type:'module',exports:'./index.js'},{'index.js':'export const value = "actual-fixture-js";'});
    pkg('@fixture/unused',{type:'module',exports:'./index.js'},{'index.js':'export const value = "never-imported";'});
    symlinkSync(realpathSync(join(repo,'node_modules/tailwindcss')),join(root,'node_modules/tailwindcss'),'dir');
    write(join(root,'index.html'),'<html><head></head><body class="fixture"><script type="module" src="/main.js"></script></body></html>');
    write(join(root,'main.js'),'import "./entry.css"; import {value} from "@fixture/js"; console.log(value);');
    write(join(root,'entry.css'),'@import "tailwindcss";\n@import "@fixture/css";');
    assert.equal(npmOwner(join(css,'index.css'))?.value.name,'@fixture/css');
    const output = await build({root,configFile:false,logLevel:'silent',plugins:[licenseInventory(),tailwindcss()],build:{write:false,assetsInlineLimit:0,minify:false,emptyOutDir:false}});
    const assets = (Array.isArray(output)?output:[output]).flatMap(result=>result.output);
    const raw = assets.find(asset=>asset.type==='asset'&&asset.fileName==='license-inventory.frontend.json');
    assert.ok(raw);
    const inventory = JSON.parse(String(raw.source));
    const style = inventory.entries.find(row=>row.name==='@fixture/css');
    const js = inventory.entries.find(row=>row.name==='@fixture/js');
    assert.ok(style,'CSS inlined by the actual compiler remains attributed');
    assert.ok(style.evidence.some(item=>item.detail.includes('CSS files registered by the actual build')));
    assert.ok(!style.evidence.some(item=>item.detail.includes('Nonzero rendered JavaScript')),'CSS-only package never masquerades as retained JavaScript');
    const font = style.emittedAssets.find(asset=>asset.kind==='woff2');
    assert.ok(font);
    const emitted = assets.find(asset=>asset.fileName===font.fileName);
    assert.equal(font.sha256,sha256(readFileSync(join(css,'actual.woff2'))));
    assert.equal(font.sha256,sha256(emitted.source));
    assert.ok(js.evidence.some(item=>item.kind==='bundled'&&item.detail.includes('Nonzero rendered JavaScript')));
    assert.ok(!inventory.entries.some(row=>row.name==='@fixture/unused'),'installation alone is no build or delivery evidence');
    assert.ok(inventory.provenance.inputs.some(input=>input.label==='resources/licenses/frontend-package-materials.json'&&input.sha256===sha256(readFileSync(join(repo,input.label)))));
    const changedHooks = tailwindcss().map(plugin=>({...plugin,name:plugin.name==='@tailwindcss/vite:generate:build'?'@tailwindcss/vite:unrecognized-build-hook':plugin.name}));
    const changedOutput = await build({root,configFile:false,logLevel:'silent',plugins:[licenseInventory(),changedHooks],build:{write:false,assetsInlineLimit:0,minify:false,emptyOutDir:false}});
    const changedAssets = (Array.isArray(changedOutput)?changedOutput:[changedOutput]).flatMap(result=>result.output);
    const changedInventory = JSON.parse(String(changedAssets.find(asset=>asset.type==='asset'&&asset.fileName==='license-inventory.frontend.json').source));
    assert.ok(changedInventory.gaps.some(gap=>gap.reason==='css-compiler-dependency-observer-unavailable'),'a changed compiler hook leaves an explicit evidence gap instead of silently losing CSS ownership');
  } finally {rmSync(root,{recursive:true,force:true});}
});
