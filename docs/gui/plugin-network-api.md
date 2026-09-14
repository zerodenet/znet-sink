# 插件网络调用（桌面，2026-09-12）

正式桌面宿主现已支持以下调用。中央登记的能力上限、签名 manifest 与用户授权必须同时包含对应能力；独立实验 `execute` 默认仍关闭网络。

```json
{
  "required": [
    { "capability": "network.request", "scope": "https://panel.example.com" }
  ]
}
```

`scope` 是规范化 origin（协议、主机、非默认端口），不是路径或通配符。下面的 JS 在已安装且获授权的 VM 内运行；账号协议、响应解释由插件实现。

```js
const response = JSON.parse(hostCall(JSON.stringify({
  capability: 'network.request',
  scope: 'https://panel.example.com',
  url: 'https://panel.example.com/api/session',
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ account: 'example', proof: 'plugin-provided' })
})));
// response: { status, headers, body, body_base64 }
response.status;
```

- method 默认 GET，支持 GET/HEAD/POST/PUT/PATCH/DELETE。GET/HEAD 不接收正文。
- body 是 UTF-8 文本；二进制使用 body_base64，二者不能同时提供。
- 请求头最多 16 个、总计 8 KiB。禁止 Host、代理和连接/传输控制头。Authorization/Cookie 如有需要由插件显式提供，宿主不提供系统 Cookie jar 或浏览器登录状态。
- 非成功 HTTP 状态也正常返回，由插件处理。响应 headers 当前仅提供 content-type/location；不能假设已提供 Set-Cookie 或所有响应头。非 UTF-8 正文的 body 为 null，body_base64 仍可用。
- 通用请求不自动跟随重定向。插件读取 location 后需要发起另一次经授权检查的调用，并自行决定是否携带业务凭据。
- `network.get` 是独立的读取权限，只接收 capability/scope/url；允许在逐跳授权检查后跟随重定向。它不能借 method/body 升级成通用请求。

当前 VM 单次执行最多 2 秒，资源与输出预算最多 64 KiB；编码后的网络响应也计入输出限制，因此可用正文小于该数值。超限/超时、权限被撤销或目标不在授权范围都会失败。撤销不等于撤回已发送的服务器操作，不能自动重放结果不明的写请求。

此接口返回插件可读的网络响应，不是秘密材料通道。当前也未实现 IP 网段/DNS 重绑定过滤。大订阅、受保护材料存储、配置投递及重启恢复仍需后续能力接入；不得用普通配置 content 或把结果显示在 UI 来模拟这些能力。
