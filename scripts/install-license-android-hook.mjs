// Explicit, job-scoped Gradle hook installation; never writes user/global configuration.
import {mkdirSync,copyFileSync,existsSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const home=process.env.GRADLE_USER_HOME;
if(!home)throw new Error('Explicit job-scoped GRADLE_USER_HOME required');
for(const name of ['ORG_GRADLE_PROJECT_znetLicenseMergeScript','ORG_GRADLE_PROJECT_znetLicenseCargoInventory','ORG_GRADLE_PROJECT_znetLicenseFrontendInventory','ORG_GRADLE_PROJECT_znetLicenseMaterialsCache'])if(!process.env[name]||!existsSync(process.env[name]))throw new Error(`Required prepared inventory/hook input ${name}`);
mkdirSync(join(resolve(home),'init.d'),{recursive:true});copyFileSync(join(root,'scripts/license-gradle.init.gradle'),join(resolve(home),'init.d/license-gradle.init.gradle'));
console.log('Installed build-scoped Android license hook; required inputs verified');
