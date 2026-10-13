// These raw imports bind each displayed text to the installed package/resource
// used by this frontend build. This is intentionally a limited inventory.
import projectText from '../../../LICENSE?raw';
import lucideText from '../../../node_modules/@lucide/svelte/LICENSE?raw';
import lucide from '../../../node_modules/@lucide/svelte/package.json';
import interText from '../../../node_modules/@fontsource-variable/inter/LICENSE?raw';
import inter from '../../../node_modules/@fontsource-variable/inter/package.json';
import flagsText from '../../../node_modules/flag-icons/LICENSE?raw';
import flags from '../../../node_modules/flag-icons/package.json';
import rulesText from '../../../src-tauri/resources/builtin-rules/LICENSE.meta-rules-dat?raw';
import rulesNotice from '../../../src-tauri/resources/builtin-rules/NOTICE.md?raw';
import rules from '../../../src-tauri/resources/builtin-rules/manifest.json';
export const projectLicense={id:'MPL-2.0',text:projectText,source:'仓库 LICENSE'} as const;
export const mobileLicenseNotices=[
  {name:lucide.name,version:lucide.version,license:lucide.license,text:lucideText,notice:'图标；版本与全文读取自本次安装的 npm 包。'},
  {name:inter.name,version:inter.version,license:inter.license,text:interText,notice:'字体；版本与全文读取自本次安装的 npm 包。'},
  {name:flags.name,version:flags.version,license:flags.license,text:flagsText,notice:'旗帜资源；版本与全文读取自本次安装的 npm 包。'},
  {name:'MetaCubeX/meta-rules-dat',version:rules.sourceCommit,license:rules.sourceLicense,text:rulesText,notice:rulesNotice},
] as const;
export const licenseInventoryGap='当前资料覆盖这里列出的前端资源及项目随附规则来源。其余 npm、Rust 和平台原生依赖的实际构建许可目录尚未完整提供。';
