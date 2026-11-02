import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    let endpoints: unknown[] = [];
    let revision = 0;
    let protocols = [{ type: 'wireguard', label: 'WireGuard' }];
    window.addEventListener('fixture-authoring-library', event => {
      const next = (event as CustomEvent<{ protocols: typeof protocols; endpoints: unknown[] }>).detail;
      protocols = next.protocols; endpoints = next.endpoints; revision++;
    });
    let hold = false;
    window.addEventListener('fixture-hold-library-save', () => { hold = true; });
    const host = window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown> } };
    host.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      if (command === 'gui_user_endpoints') return { revision: String(revision), endpoints, protocols };
      if (command === 'gui_user_endpoints_save') {
        window.dispatchEvent(new CustomEvent('fixture-library-save-attempt', { detail: args }));
        if (hold) await new Promise<void>(resolve => window.addEventListener('fixture-release-library-save', () => { hold = false; resolve(); }, { once: true }));
        if (args?.expectedRevision !== String(revision)) throw new Error('用户端点已变化，请刷新后重新编辑');
        const rows = args?.endpoints as { tag: string }[];
        if (rows.some(row => row.tag === 'conflicting')) throw new Error('用户端点与配置名称冲突');
        endpoints = rows; revision++;
        return { revision: String(revision), endpoints, protocols };
      }
      throw new Error(`Unexpected command ${command}`);
    } };
    window.addEventListener('fixture-library-external-change', () => { revision++; });
  });
  await page.goto('/?panel=endpoints-shell&tab=endpoints');
});

async function fillEndpoint(page: import('@playwright/test').Page, tag = 'my-wg') {
  await page.getByRole('button', { name: '创建端点', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '创建端点', exact: true });
  await dialog.getByRole('textbox', { name: '端点名称', exact: true }).fill(tag);
  await dialog.getByLabel(/^私钥/).fill('AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=');
  await dialog.getByRole('textbox', { name: '本机隧道地址', exact: true }).fill('10.66.0.2/32, fd66::2/128');
  await dialog.getByRole('textbox', { name: '公钥', exact: true }).fill('AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=');
  await dialog.getByLabel('远端地址', { exact: true }).fill('vpn.example:51820');
  return dialog;
}
async function cardMenu(page: import('@playwright/test').Page, tag: string, action: string) {
  const card = page.getByRole('article', { name: `端点 ${tag}`, exact: true });
  await card.getByLabel(`${tag} 更多操作`, { exact: true }).click();
  await card.getByRole('button', { name: action, exact: true }).click();
}

test('one endpoint list supports creation and card editing deletion through dialogs', async ({ page }) => {
  let dialog = await fillEndpoint(page);
  await expect(page.getByRole('region', { name: '全局用户端点' })).toHaveCount(0);
  await expect(dialog.getByLabel(/^私钥/)).toHaveAttribute('type', 'password');
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('article', { name: '端点 my-wg', exact: true })).toHaveCount(1);
  await expect(page.getByLabel(/^私钥/)).toHaveCount(0);
  await cardMenu(page, 'my-wg', '编辑端点');
  dialog = page.getByRole('dialog', { name: '编辑端点', exact: true });
  await expect(dialog.getByRole('textbox', { name: '本机隧道地址', exact: true })).toHaveValue('10.66.0.2/32, fd66::2/128');
  await dialog.getByRole('button', { name: '添加 Peer', exact: true }).click();
  await dialog.getByRole('textbox', { name: '公钥', exact: true }).nth(1).fill('AwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwM=');
  await dialog.getByRole('textbox', { name: '允许的 IP', exact: true }).nth(1).fill('10.67.0.0/16');
  await dialog.getByLabel('远端地址', { exact: true }).nth(1).fill('peer-two.example:51820');
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await cardMenu(page, 'my-wg', '编辑端点');
  await expect(dialog.getByRole('textbox', { name: '公钥', exact: true })).toHaveCount(2);
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByRole('tab', { name: '节点', exact: true }).click();
  await page.getByRole('tab', { name: '端点', exact: true }).click();
  await cardMenu(page, 'my-wg', '删除端点');
  const confirmation = page.getByRole('dialog', { name: '删除端点 my-wg？', exact: true });
  await confirmation.getByRole('button', { name: '确认删除', exact: true }).click();
  await expect(page.getByRole('article', { name: '端点 my-wg', exact: true })).toHaveCount(0);
  const source = page.getByRole('article', { name: '端点 wg-a', exact: true });
  await source.getByLabel('wg-a 更多操作').click();
  await expect(source.getByRole('button', { name: '删除端点', exact: true })).toHaveCount(0);
});

test('rejected save retains the dialog draft without success or automatic replay', async ({ page }) => {
  const dialog = await fillEndpoint(page, 'conflicting');
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await expect(dialog.getByRole('alert')).toContainText('名称冲突');
  await expect(dialog.getByRole('textbox', { name: '端点名称', exact: true })).toHaveValue('conflicting');
  await page.evaluate(() => window.dispatchEvent(new Event('fixture-library-external-change')));
  await dialog.getByRole('textbox', { name: '端点名称', exact: true }).fill('my-wg');
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await expect(dialog.getByRole('alert')).toContainText('已变化');
  await expect(page.getByRole('article', { name: '端点 my-wg', exact: true })).toHaveCount(0);
});

test('long peer forms scroll inside the dialog while save and cancel remain visible', async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 650 });
  const dialog = await fillEndpoint(page);
  for (let i = 0; i < 3; i++) await dialog.getByRole('button', { name: '添加 Peer', exact: true }).click();
  const dimensions = await dialog.evaluate(element => {
    const body = element.querySelector('[data-slot="dialog-body"]')!;
    const footer = element.querySelector('[data-slot="dialog-footer"]')!.getBoundingClientRect();
    return { scrolls: body.scrollHeight > body.clientHeight, footerTop: footer.top, footerBottom: footer.bottom, right: element.getBoundingClientRect().right };
  });
  expect(dimensions.scrolls).toBe(true);
  expect(dimensions.footerTop).toBeGreaterThan(0);
  expect(dimensions.footerBottom).toBeLessThanOrEqual(650);
  expect(dimensions.right).toBeLessThanOrEqual(420);
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: '创建端点', exact: true })).toBeFocused();
});

test('saving blocks dialog dismissal until the response is confirmed', async ({ page }) => {
  const dialog = await fillEndpoint(page);
  await page.evaluate(() => window.dispatchEvent(new Event('fixture-hold-library-save')));
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await expect(dialog.getByRole('button', { name: '正在保存…' })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event('fixture-release-library-save')));
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('article', { name: '端点 my-wg', exact: true })).toBeVisible();
});


test('a confirmed user endpoint reuses its runtime card and exposes editing and deletion there', async ({ page }) => {
  await page.goto('/?panel=endpoints-shell&tab=endpoints&preview=1');
  // Use the interactive preview library for this kernel+library integration fixture.
  await page.evaluate(() => { delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__; });
  await page.getByRole('button', { name: '刷新端点', exact: true }).click();
  const card = page.getByRole('article', { name: '端点 wg-home', exact: true });
  await expect(card).toHaveCount(1);
  await expect(card.getByRole('switch')).toBeVisible();
  await cardMenu(page, 'wg-home', '编辑端点');
  const dialog = page.getByRole('dialog', { name: '编辑端点', exact: true });
  await expect(dialog.getByRole('textbox', { name: '端点名称', exact: true })).toHaveValue('wg-home');
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await cardMenu(page, 'wg-home', '删除端点');
  await page.getByRole('dialog', { name: '删除端点 wg-home？', exact: true }).getByRole('button', { name: '确认删除', exact: true }).click();
  await expect(card).toHaveCount(0);
});


test('protocol choices use backend authoring support and unknown definitions never open WireGuard fields', async ({ page }) => {
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('fixture-authoring-library', { detail: {
    protocols: [{ type: 'wireguard', label: 'WireGuard' }, { type: 'future-mesh', label: 'Future Mesh' }],
    endpoints: [{ tag: 'mesh-local', protocol: { type: 'future-mesh', options: { opaque: true } } }],
  } })));
  await page.getByRole('button', { name: '刷新端点', exact: true }).click();
  await expect(page.getByRole('article', { name: '端点 mesh-local', exact: true })).toContainText('future-mesh');
  await page.getByRole('button', { name: '创建端点', exact: true }).click();
  let dialog = page.getByRole('dialog', { name: '创建端点', exact: true });
  await dialog.getByRole('button', { name: '协议', exact: true }).click();
  await expect(page.getByRole('option')).toHaveCount(1);
  await expect(page.getByRole('option', { name: 'WireGuard', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await cardMenu(page, 'mesh-local', '编辑端点');
  dialog = page.getByRole('dialog', { name: '编辑端点', exact: true });
  await expect(dialog.getByRole('alert')).toContainText('尚无此协议的编辑器');
  await expect(dialog.getByLabel(/^私钥/)).toHaveCount(0);
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
});

test('protocol form preserves typed CIDR separators and serializes multiple addresses', async ({ page }) => {
  let dialog = await fillEndpoint(page);
  const addresses = dialog.getByRole('textbox', { name: '本机隧道地址', exact: true });
  await addresses.fill('10.66.0.2/32');
  await addresses.press('End');
  await addresses.pressSequentially(', fd66::2/128');
  await expect(addresses).toHaveValue('10.66.0.2/32, fd66::2/128');
  const allowed = dialog.getByRole('textbox', { name: '允许的 IP', exact: true });
  await allowed.fill('10.66.0.0/16');
  await allowed.press('End');
  await allowed.pressSequentially(', fd66::/64');
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await cardMenu(page, 'my-wg', '编辑端点');
  dialog = page.getByRole('dialog', { name: '编辑端点', exact: true });
  await expect(dialog.getByRole('textbox', { name: '本机隧道地址', exact: true })).toHaveValue('10.66.0.2/32, fd66::2/128');
  await expect(dialog.getByRole('textbox', { name: '允许的 IP', exact: true })).toHaveValue('10.66.0.0/16, fd66::/64');
});

const importConf = `[Interface]
PrivateKey = AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=
Address = 10.66.0.2/32, fd66::2/128
DNS = 1.1.1.1
MTU = 1380
[Peer]
PublicKey = AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=
PresharedKey = AwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwM=
Endpoint = vpn.example:51820
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25
`;
const configFile = (name: string, content = importConf) => ({ name, mimeType: 'text/plain', buffer: Buffer.from(content) });

test('invalid tunnel addresses identify and focus the field without attempting a save', async ({ page }) => {
  const dialog = await fillEndpoint(page);
  await page.evaluate(() => {
    document.body.dataset.endpointSaveAttempts = '0';
    window.addEventListener('fixture-library-save-attempt', () => { document.body.dataset.endpointSaveAttempts = String(Number(document.body.dataset.endpointSaveAttempts) + 1); });
  });
  const addresses = dialog.getByRole('textbox', { name: '本机隧道地址', exact: true });
  await addresses.fill('AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=');
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await expect(addresses).toBeFocused();
  await expect(addresses).toHaveAttribute('aria-invalid', 'true');
  await expect(dialog.getByRole('alert')).toContainText('这里不能填写密钥');
  await expect(dialog).toContainText('不是私钥或公钥');
  await expect(page.locator('body')).toHaveAttribute('data-endpoint-save-attempts', '0');
  await expect(page.getByRole('article', { name: '端点 my-wg', exact: true })).toHaveCount(0);
});

test('enabling inbound exposes listener configuration and persists it', async ({ page }) => {
  let dialog = await fillEndpoint(page);
  await expect(dialog.getByRole('textbox', { name: '监听端口', exact: true })).toHaveCount(0);
  await dialog.getByRole('switch', { name: '入站', exact: true }).click();
  await dialog.getByRole('textbox', { name: '监听地址', exact: true }).fill('127.0.0.1');
  await dialog.getByRole('textbox', { name: '监听端口', exact: true }).fill('51821');
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await expect(dialog).toHaveCount(0);
  await cardMenu(page, 'my-wg', '编辑端点');
  dialog = page.getByRole('dialog', { name: '编辑端点', exact: true });
  await expect(dialog.getByRole('textbox', { name: '监听地址', exact: true })).toHaveValue('127.0.0.1');
  await expect(dialog.getByRole('textbox', { name: '监听端口', exact: true })).toHaveValue('51821');
});

test('file filling replaces visible fields, preserves the name and retains optional connection settings', async ({ page }) => {
  let dialog = await fillEndpoint(page, 'custom-name');
  await dialog.getByRole('textbox', { name: '本机隧道地址', exact: true }).fill('wrong-address');
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await dialog.getByLabel('从文件填写 WireGuard', { exact: true }).setInputFiles(configFile('home.conf'));
  await expect(dialog.getByRole('alert')).toHaveCount(0);
  await expect(dialog.getByRole('textbox', { name: '端点名称', exact: true })).toHaveValue('custom-name');
  await expect(dialog.getByLabel('MTU', { exact: true })).toHaveValue('1380');
  await expect(dialog.getByLabel('保活间隔（秒）', { exact: true })).toHaveValue('25');
  await expect(dialog.getByLabel('预共享密钥', { exact: true })).toHaveValue('AwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwM=');
  await expect(dialog.getByRole('textbox', { name: 'DNS 服务器', exact: true })).toHaveValue('1.1.1.1');
  await expect(dialog.getByRole('switch', { name: '用作默认 DNS', exact: true })).toBeChecked();
  await dialog.getByRole('button', { name: '保存并应用' }).click();
  await cardMenu(page, 'custom-name', '编辑端点');
  dialog = page.getByRole('dialog', { name: '编辑端点', exact: true });
  await expect(dialog.getByLabel('MTU', { exact: true })).toHaveValue('1380');
  await expect(dialog.getByLabel('保活间隔（秒）', { exact: true })).toHaveValue('25');
});

test('multiple files preview without saving, resolve collisions, and import all peers together', async ({ page }) => {
  await page.getByRole('button', { name: '导入配置', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '创建端点', exact: true });
  const multiplePeers = importConf + `[Peer]
PublicKey = BAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQ=
Endpoint = peer-two.example:51820
AllowedIPs = 10.67.0.0/16
`;
  await dialog.getByLabel('选择 WireGuard 配置文件', { exact: true }).setInputFiles([configFile('home.conf', multiplePeers), configFile('home.ini')]);
  await expect(dialog.getByRole('article')).toHaveCount(2);
  await expect(dialog.getByRole('article').first()).toContainText('2 个 Peer');
  await expect(dialog.getByRole('button', { name: '确认导入并应用' })).toBeDisabled();
  await expect(page.getByRole('article', { name: '端点 home', exact: true })).toHaveCount(0);
  await dialog.getByRole('textbox', { name: '端点名称', exact: true }).nth(1).fill('office');
  await dialog.getByRole('button', { name: '确认导入并应用' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('article', { name: '端点 home', exact: true })).toBeVisible();
  await expect(page.getByRole('article', { name: '端点 office', exact: true })).toBeVisible();
  await cardMenu(page, 'home', '编辑端点');
  await expect(page.getByRole('dialog').getByRole('textbox', { name: '公钥', exact: true })).toHaveCount(2);
});

test('a malformed file blocks the batch without exposing secret data or saving valid siblings', async ({ page }) => {
  await page.getByRole('button', { name: '导入配置', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '创建端点', exact: true });
  await dialog.getByLabel('选择 WireGuard 配置文件', { exact: true }).setInputFiles([configFile('good.conf'), configFile('bad.conf', importConf.replace('10.66.0.2/32, fd66::2/128', 'SECRET'))]);
  await expect(dialog.getByRole('alert')).toContainText('IP/前缀');
  await expect(dialog.getByRole('button', { name: '确认导入并应用' })).toBeDisabled();
  await expect(dialog).not.toContainText('SECRET');
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await expect(page.getByRole('article', { name: '端点 good', exact: true })).toHaveCount(0);
});

test('failed batch save retains preview and never claims success or retries automatically', async ({ page }) => {
  await page.getByRole('button', { name: '导入配置', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '创建端点', exact: true });
  await dialog.getByLabel('选择 WireGuard 配置文件', { exact: true }).setInputFiles(configFile('home.conf'));
  await page.evaluate(() => window.dispatchEvent(new Event('fixture-library-external-change')));
  await dialog.getByRole('button', { name: '确认导入并应用' }).click();
  await expect(dialog.getByRole('alert')).toContainText('已变化');
  await expect(dialog.getByRole('textbox', { name: '端点名称', exact: true })).toHaveValue('home');
  await expect(page.getByRole('article', { name: '端点 home', exact: true })).toHaveCount(0);
});


test('create entry parses pasted configuration and previews DNS before any save', async ({ page }) => {
  await page.getByRole('button', { name: '创建端点', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '创建端点', exact: true });
  await dialog.getByLabel('创建端点方式', { exact: true }).getByText('粘贴配置', { exact: true }).click();
  await dialog.getByRole('textbox', { name: '端点名称', exact: true }).fill('pasted-home');
  await dialog.getByLabel('WireGuard 配置文本', { exact: true }).fill(importConf.replace('DNS = 1.1.1.1', 'DNS = 10.66.0.1, fd66::53'));
  await dialog.getByRole('button', { name: '解析并预览', exact: true }).click();
  const preview = dialog.getByRole('article');
  await expect(preview).toContainText('10.66.0.1, fd66::53');
  await expect(preview.getByRole('switch')).toBeChecked();
  await expect(page.getByRole('article', { name: '端点 pasted-home', exact: true })).toHaveCount(0);
  await dialog.getByRole('button', { name: '确认导入并应用', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await cardMenu(page, 'pasted-home', '编辑端点');
  const edit = page.getByRole('dialog', { name: '编辑端点', exact: true });
  await expect(edit.getByRole('textbox', { name: 'DNS 服务器', exact: true })).toHaveValue('10.66.0.1, fd66::53');
  await expect(edit.getByRole('switch', { name: '用作默认 DNS', exact: true })).toBeChecked();
});

test('create and toolbar shortcuts share the same file preview and choose a single DNS source', async ({ page }) => {
  await page.getByRole('button', { name: '创建端点', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '创建端点', exact: true });
  await dialog.getByLabel('创建端点方式', { exact: true }).getByText('导入文件', { exact: true }).click();
  await dialog.getByLabel('选择 WireGuard 配置文件', { exact: true }).setInputFiles([configFile('home.conf'), configFile('office.conf', importConf.replace('1.1.1.1', '10.67.0.1'))]);
  await expect(dialog.getByRole('article').nth(0).getByRole('switch')).toBeChecked();
  await expect(dialog.getByRole('article').nth(1).getByRole('switch')).not.toBeChecked();
  await dialog.getByRole('article').nth(1).getByRole('switch').click();
  await expect(dialog.getByRole('article').nth(0).getByRole('switch')).not.toBeChecked();
  await dialog.getByRole('button', { name: '确认导入并应用', exact: true }).click();
  await cardMenu(page, 'home', '编辑端点');
  await expect(page.getByRole('dialog').getByRole('switch', { name: '用作默认 DNS', exact: true })).not.toBeChecked();
  await page.getByRole('dialog').getByRole('button', { name: '取消', exact: true }).click();
  await cardMenu(page, 'office', '编辑端点');
  await expect(page.getByRole('dialog').getByRole('switch', { name: '用作默认 DNS', exact: true })).toBeChecked();
});

test('invalid pasted input retains the text for correction and creates nothing', async ({ page }) => {
  await page.getByRole('button', { name: '创建端点', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '创建端点', exact: true });
  await dialog.getByLabel('创建端点方式', { exact: true }).getByText('粘贴配置', { exact: true }).click();
  const text = dialog.getByLabel('WireGuard 配置文本', { exact: true });
  const invalid = importConf.replace('DNS = 1.1.1.1','DNS = bad/domain');
  await text.fill(invalid);
  await dialog.getByRole('button', { name: '解析并预览', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('DNS');
  await expect(text).toHaveValue(invalid);
  await expect(dialog.getByRole('button', { name: '确认导入并应用', exact: true })).toBeDisabled();
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await expect(page.getByRole('article', { name: '端点 粘贴配置', exact: true })).toHaveCount(0);
});
