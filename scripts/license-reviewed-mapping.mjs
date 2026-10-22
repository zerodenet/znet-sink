// Explicitly reviewed package text, never a general SPDX inference rule.
export const svelteToolbeltReview = Object.freeze({
  identity: {ecosystem:'npm',name:'svelte-toolbelt',version:'0.10.6',source:'npm:svelte-toolbelt@0.10.6'},
  expected: {manifestSha256:'b0dd8c15c05e68d20b0642f9bfd72ef07740a8e6fa8e4838baeed9cb7d0ef69b',lockIntegrity:'sha512-YWuX+RE+CnWYx09yseAe4ZVMM7e7GRFZM6OYWpBKOb++s+SQ8RBIMMe+Bs/CznBMc0QPLjr+vDBxTAkozXsFXQ=='},
  archiveSha256:'37e3a20a2a45bd494329279c57a157356b9103bc9fe5059fca23aecef39895ca',
  materialSha256:'8914f144af4333312a07b30438dc6b75dd8da7a665c3ae9fa751f229eb615e8b'
});
/** @param {any} pkg */
export function validateReviewedLicenseMapping(pkg) {
  const mapping=pkg.reviewedLicenseMapping;
  if(mapping===undefined)return;
  const review=svelteToolbeltReview;
  if(!mapping||mapping.expression!=='MIT'||mapping.basis!=='reviewed-package-license-text'||mapping.manifestDeclared!==false||mapping.materialSha256!==review.materialSha256
    ||Object.entries(review.identity).some(([key,value])=>pkg.identity?.[key]!==value)
    ||Object.entries(review.expected).some(([key,value])=>pkg.expected?.[key]!==value)
    ||!pkg.materials.some(/** @param {any} m */ m=>m.kind==='license'&&m.sha256===review.materialSha256&&m.origin?.sha256===review.archiveSha256&&m.origin?.member==='package/LICENSE'))throw new Error('Unreviewed package license mapping');
}
/** @param {any} row @param {any} pkg */
export function applyReviewedLicenseMapping(row,pkg) {
  if(pkg.reviewedLicenseMapping===undefined)return;
  validateReviewedLicenseMapping(pkg);
  // Require the installed package itself to supply the reviewed text before
  // adding cached material. A cache must not conceal a locally changed LICENSE.
  if(row.declaredLicense!=null||!row.licenseTextIds?.includes(`sha256-${svelteToolbeltReview.materialSha256}`))throw new Error('Reviewed package license text or manifest changed');
  row.reviewedLicenseMapping=structuredClone(pkg.reviewedLicenseMapping);
  row.evidence.push({kind:'reviewed-license-mapping',detail:'MIT mapped from this exact published package LICENSE after review. Its package manifest does not declare a license; the declaration gap remains.',source:pkg.materials.find(/** @param {any} m */ m=>m.sha256===svelteToolbeltReview.materialSha256).source,sha256:svelteToolbeltReview.materialSha256});
}
