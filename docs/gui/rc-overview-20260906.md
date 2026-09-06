# 专业概览 RC 发布记录

日期：2026-09-06。状态：四平台安装包已公开发布，更新清单校验通过。

| 项目 | 值 |
| --- | --- |
| 客户端版本 | `0.0.16-rc.202609060937` |
| 原生构建号 | `360` |
| 功能提交 | `7a09357f7c773f5e2b07b3be4ff16facdbf5af86` |
| 发布提交 | `f9996a464eac643e8543b840b6530d40b9286f90` |
| 标签 | `v0.0.16-rc.202609060937` |

主分支和不可变 RC 标签已同步到 GitLab origin 与 GitHub。版本尾部采用 UTC 时间；本轮只发布客户端，没有修改或重新发布 Zero 内核。

## 本次内容

- 按确认稿落实专业概览：保留应用 Logo、窗口、导航与快捷开关，以配置、内核、模式/策略、TUN、网络检查和流量组织能力。
- 配置、策略组、运行模式、TUN 与系统代理调用已有真实接口；自动策略组保持只读，手动组可选择直接成员。
- 操作等待内核确认，失败和回读不确定分别反馈；保留状态、策略和流量采样的过期保护。
- 简约模式沿用原界面，并减少专业模式下重复读取简约模式配置来源。

实现与设计核查见 [专业概览能力组织校正](./overview-capability-review-20260906.md)。

## 验证

- 本地 `pnpm verify` 完整通过：前端回归、类型检查、生产构建、Rust 格式/编译与测试。Rust 共 477 项通过，4 项按原测试定义忽略。
- 概览模型和命令回归共 17 项通过。
- [功能提交 CI](https://github.com/zerodenet/znet-sink/actions/runs/34024931998) 的 Chromium/WebKit 共 42 项交互测试通过，Windows 客户端、前端产品、macOS/Linux 下载恢复检查通过。
- [发布提交 CI](https://github.com/zerodenet/znet-sink/actions/runs/34025286439) 全部通过，包含前端产品回归、Chromium/WebKit、Windows 客户端与 Zero DNS 合约、macOS/Linux 下载恢复检查。
- [发布工作流](https://github.com/zerodenet/znet-sink/actions/runs/34025286709) 全部成功：Windows x64、macOS ARM64/Intel、Linux x64 构建及最终发布步骤通过。
- 公开预发布包含 23 个附件。`latest.json` 版本匹配本轮 RC，Windows NSIS、macOS ARM/Intel、Linux 四个更新入口均指向本轮已上传的非空附件；签名字段非空且符合预期编码格式。

浏览器与命令测试使用模拟适配器，不能替代安装客户端后的原生网络验收。本轮没有安装或重启本机客户端，也没有更改本机系统代理/TUN。

## 下载

[客户端 RC 发布页](https://github.com/zerodenet/znet-sink/releases/tag/v0.0.16-rc.202609060937)。公开时间：北京时间 2026-09-06 17:52:07（UTC 09:52:07），保留 prerelease 标记。

[macOS Intel DMG](https://github.com/zerodenet/znet-sink/releases/download/v0.0.16-rc.202609060937/ZNet.Sink_0.0.16-rc.202609060937_x64.dmg) · [macOS ARM64 DMG](https://github.com/zerodenet/znet-sink/releases/download/v0.0.16-rc.202609060937/ZNet.Sink_0.0.16-rc.202609060937_aarch64.dmg)。Windows 与 Linux 安装包见发布页。
