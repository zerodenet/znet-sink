# Git 提交规范 (Conventional Commits)

## 提交格式

```
<type>(<scope>): <subject>
<BLANK LINE>
<body>
<BLANK LINE>
<footer>
```

## Type 类型说明

| 类型 | 说明 |
|------|------|
| `feat` | 新功能 |
| `fix` | 修复 bug |
| `docs` | 文档更新 |
| `style` | 代码风格（不影响代码运行的变动） |
| `refactor` | 重构（不是新增功能，也不是修复 bug） |
| `perf` | 性能优化 |
| `test` | 测试相关 |
| `build` | 构建系统、依赖变更 |
| `ci` | CI 配置变更 |
| `chore` | 其他不修改源码的变动 |
| `revert` | 回滚提交 |

## 规范说明

### Header (标题行)
- **type**: 必填，说明提交类型
- **scope**: 可选，说明影响范围（如：components, router, api）
- **subject**: 必填，简短描述
  - 使用祈使句，小写开头
  - 不使用句号结尾
  - 不超过 72 字符

### Body (正文)
- 可选，详细描述提交内容
- 每行不超过 100 字符
- 说明变更动机和对比

### Footer (页脚)
- 可选，用于关闭 Issue 或 BREAKING CHANGE
- 格式: `Closes #123` 或 `Fixes #456`

## 使用方式

### 方式一：交互式提交（推荐）
```bash
pnpm commit
```

### 方式二：手动提交
```bash
git commit -m "feat(auth): add login functionality"
```

### 验证提交信息
```bash
pnpm lint:commit
```

## 示例

### 新功能
```
feat(user): add profile edit page

- implement form validation
- add avatar upload feature
- connect to user API

Closes #42
```

### Bug 修复
```
fix(login): correct password validation logic

The previous regex allowed empty passwords. Updated to require
minimum 8 characters with at least one number and symbol.

Fixes #123
```

### 破坏性变更
```
refactor(api): change authentication endpoint

BREAKING CHANGE: The /api/auth endpoint has been moved to /api/v2/auth.
Update your API base URLs accordingly.
```
