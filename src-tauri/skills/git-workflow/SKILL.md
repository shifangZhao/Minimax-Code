---
name: git-workflow
description: Git 工作流模式，包括分支策略、提交约定、merge vs rebase、冲突解决和各种规模团队协作开发的最佳实践。
origin: ECC
---

# Git 工作流模式

Git 版本控制、分支策略和协作开发的最佳实践。

## 激活时机

- 为新项目设置 Git 工作流
- 决定分支策略（GitFlow、基于 trunk、GitHub flow）
- 编写提交消息和 PR 描述
- 解决合并冲突
- 管理发布和版本标签
- 让新团队成员熟悉 Git 实践

## 分支策略

### GitHub Flow（简单，推荐大多数情况）

最适合持续部署和中小型团队。

```
main (protected，始终可部署)
  │
  ├── feature/user-auth      → PR → 合并到 main
  ├── feature/payment-flow   → PR → 合并到 main
  └── fix/login-bug          → PR → 合并到 main
```

**规则：**
- `main` 始终可部署
- 从 `main` 创建功能分支
- 准备好审查时打开 Pull Request
- 批准且 CI 通过后合并到 `main`
- 合并后立即部署

### 基于 trunk 的开发（高 velocity 团队）

最适合有强 CI/CD 和功能标志的团队。

```
main (trunk)
  │
  ├── 短寿命功能分支（最多 1-2 天）
  ├── 短寿命功能分支
  └── 短寿命功能分支
```

**规则：**
- 每个人提交到 `main` 或非常短寿的分支
- 功能标志隐藏未完成的工作
- CI 必须通过才能合并
- 每天多次部署

### GitFlow（复杂，发布周期驱动）

最适合有计划发布和企业项目。

```
main (生产发布)
  │
  └── develop (集成分支)
        │
        ├── feature/user-auth
        ├── feature/payment
        │
        ├── release/1.0.0    → 合并到 main 和 develop
        │
        └── hotfix/critical  → 合并到 main 和 develop
```

**规则：**
- `main` 仅包含生产就绪代码
- `develop` 是集成分支
- 功能分支从 `develop` 创建，合并回 `develop`
- 发布分支从 `develop` 创建，合并到 `main` 和 `develop`
- 热修复分支从 `main` 创建，合并到 `main` 和 `develop`

### 何时使用哪种

| 策略 | 团队规模 | 发布节奏 | 最适合 |
|----------|-----------|-----------------|----------|
| GitHub Flow | 任意 | 持续 | SaaS、web 应用、创业公司 |
| 基于 trunk | 5+ 有经验 | 多次/天 | 高 velocity 团队、功能标志 |
| GitFlow | 10+ | 计划 | 企业、受监管行业 |

## 提交消息

### 约定提交格式

```
<类型>(<范围>): <主题>

[可选正文]

[可选页脚]
```

### 类型

| 类型 | 用于 | 示例 |
|------|---------|---------|
| `feat` | 新功能 | `feat(auth): add OAuth2 login` |
| `fix` | Bug 修复 | `fix(api): handle null response in user endpoint` |
| `docs` | 文档 | `docs(readme): update installation instructions` |
| `style` | 格式化，无代码变更 | `style: fix indentation in login component` |
| `refactor` | 代码重构 | `refactor(db): extract connection pool to module` |
| `test` | 添加/更新测试 | `test(auth): add unit tests for token validation` |
| `chore` | 维护任务 | `chore(deps): update dependencies` |
| `perf` | 性能改进 | `perf(query): add index to users table` |
| `ci` | CI/CD 变更 | `ci: add PostgreSQL service to test workflow` |
| `revert` | 撤销之前的提交 | `revert: revert "feat(auth): add OAuth2 login"` |

### 好与坏的示例

```
# 坏：模糊，无上下文
git commit -m "fixed stuff"
git commit -m "updates"
git commit -m "WIP"

# 好：清晰、具体、解释原因
git commit -m "fix(api): retry requests on 503 Service Unavailable

The external API occasionally returns 503 errors during peak hours.
Added exponential backoff retry logic with max 3 attempts.

Closes #123"
```

### 提交消息模板

在仓库根目录创建 `.gitmessage`：

```
# <类型>(<范围>): <主题>
# # 类型：feat, fix, docs, style, refactor, test, chore, perf, ci, revert
# 范围：api, ui, db, auth 等。
# 主题：祈使语气，无句点，最多 50 字符
#
# [可选正文] - 解释原因，而非什么
# [可选页脚] - 破坏性变更，关闭 #issue
```

启用：`git config commit.template .gitmessage`

## Merge vs Rebase

### Merge（保留历史）

```bash
# 创建合并提交
git checkout main
git merge feature/user-auth

# 结果：
# *   合并提交
# |\
# | * 功能提交
# |/
# * main 提交
```

**使用时：**
- 将功能分支合并到 `main`
- 你想保留确切历史
- 多人在分支上工作
- 分支已推送且其他人可能基于其工作

### Rebase（线性历史）

```bash
# 将功能提交重写到目标分支
git checkout feature/user-auth
git rebase main

# 结果：
# * 功能提交（重写）
# * main 提交
```

**使用时：**
- 用最新的 `main` 更新本地功能分支
- 你想要线性、干净的历史
- 分支仅本地（未推送）
- 你是唯一在分支上工作的人

### Rebase 工作流

```bash
# PR 前用最新的 main 更新功能分支
git checkout feature/user-auth
git fetch origin
git rebase origin/main

# 解决任何冲突
# 测试仍应通过

# 强制推送（仅当你唯一贡献者时）
git push --force-with-lease origin feature/user-auth
```

### 何时不 Rebase

```
# 绝不 rebase 分支：
- 已推送到共享仓库
- 其他人已基于其工作
- 受保护分支（main、develop）
- 已合并

# 为什么：Rebase 重写历史，破坏他人工作
```

## Pull Request 工作流

### PR 标题格式

```
<类型>(<范围>): <描述>

示例：
feat(auth): add SSO support for enterprise users
fix(api): resolve race condition in order processing
docs(api): add OpenAPI specification for v2 endpoints
```

### PR 描述模板

```markdown
## 做什么

这个 PR 做了什么。

## 为什么

解释动机和背景。

## 怎么做

值得强调的关键实现细节。

## 测试

- [ ] 已添加/更新单元测试
- [ ] 已添加/更新集成测试
- [ ] 已执行手动测试

## 截图（如适用）

UI 变更的前后截图。

## 检查清单

- [ ] 代码遵循项目风格指南
- [ ] 自我审查已完成
- [ ] 为复杂逻辑添加了注释
- [ ] 文档已更新
- [ ] 未引入新警告
- [ ] 测试在本地通过
- [ ] 相关问题已链接

Closes #123
```

### 代码审查检查清单

**审查者：**

- [ ] 代码是否解决了所述问题？
- [ ] 是否有未处理的边界情况？
- [ ] 代码是否易读和可维护？
- [ ] 是否有足够的测试？
- [ ] 是否有安全问题？
- [ ] 提交历史是否干净（需要时 squash）？

**作者：**

- [ ] 请求审查前已完成自我审查
- [ ] CI 通过（测试、lint、类型检查）
- [ ] PR 大小合理（<500 行理想）
- [ ] 与单一功能/修复相关
- [ ] 描述清楚解释变更

## 冲突解决

### 识别冲突

```bash
# 合并前检查冲突
git checkout main
git merge feature/user-auth --no-commit --no-ff

# 如果有冲突，Git 会显示：
# CONFLICT (content): Merge conflict in src/auth/login.ts
# Automatic merge failed; fix conflicts and then commit the result.
```

### 解决冲突

```bash
# 查看有冲突的文件
git status

# 查看文件中的冲突标记
# <<<<<<< HEAD
# main 的内容
# =======
# 功能分支的内容
# >>>>>>> feature/user-auth

# 选项 1：手动解决
# 编辑文件，移除标记，保留正确内容

# 选项 2：使用合并工具
git mergetool

# 选项 3：接受某一端
git checkout --ours src/auth/login.ts    # 保留 main 版本
git checkout --theirs src/auth/login.ts  # 保留功能版本

# 解决后，暂存并提交
git add src/auth/login.ts
git commit
```

### 冲突预防策略

```bash
# 1. 保持功能分支小且短寿
# 2. 频繁 rebase 到 main
git checkout feature/user-auth
git fetch origin
git rebase origin/main

# 3. 与团队沟通关于触碰共享文件
# 4. 使用功能标志而非长寿命分支
# 5. 及时审查和合并 PR
```

## 分支管理

### 命名约定

```
# 功能分支
feature/user-authentication
feature/JIRA-123-payment-integration

# Bug 修复
fix/login-redirect-loop
fix/456-null-pointer-exception

# 热修复（生产问题）
hotfix/critical-security-patch
hotfix/database-connection-leak

# 发布
release/1.2.0
release/2024-01-hotfix

# 实验/POC
experiment/new-caching-strategy
poc/graphql-migration
```

### 分支清理

```bash
# 删除已合并的本地分支
git branch --merged main | grep -v "^\*\|main" | xargs -n 1 git branch -d

# 删除已删除远程分支的远程追踪引用
git fetch -p

# 删除本地分支
git branch -d feature/user-auth  # 安全删除（仅在已合并时）
git branch -D feature/user-auth  # 强制删除

# 删除远程分支
git push origin --delete feature/user-auth
```

### Stash 工作流

```bash
# 保存进行中的工作
git stash push -m "WIP: user authentication"

# 列出 stash
git stash list

# 应用最近的 stash
git stash pop

# 应用特定的 stash
git stash apply stash@{2}

# 删除 stash
git stash drop stash@{0}
```

## 发布管理

### 语义化版本

```
MAJOR.MINOR.PATCH

MAJOR：破坏性变更
MINOR：新功能，向后兼容
PATCH：Bug 修复，向后兼容

示例：
1.0.0 → 1.0.1 (patch: bug 修复)
1.0.1 → 1.1.0 (minor: 新功能)
1.1.0 → 2.0.0 (major: 破坏性变更)
```

### 创建发布

```bash
# 创建带注释的标签
git tag -a v1.2.0 -m "Release v1.2.0

Features:
- Add user authentication
- Implement password reset

Fixes:
- Resolve login redirect issue

Breaking Changes:
- None"

# 推送标签到远程
git push origin v1.2.0

# 列出标签
git tag -l

# 删除标签
git tag -d v1.2.0
git push origin --delete v1.2.0
```

### Changelog 生成

```bash
# 从提交生成 changelog
git log v1.1.0..v1.2.0 --oneline --no-merges

# 或使用 conventional-changelog
npx conventional-changelog -i CHANGELOG.md -s
```

## Git 配置

### 基本配置

```bash
# 用户身份
git config --global user.name "Your Name"
git config --global user.email "your@email.com"

# 默认分支名
git config --global init.defaultBranch main

# Pull 行为（rebase 而非 merge）
git config --global pull.rebase true

# Push 行为（仅推送当前分支）
git config --global push.default current

# 自动纠正拼写错误
git config --global help.autocorrect 1

# 更好的 diff 算法
git config --global diff.algorithm histogram

# 彩色输出
git config --global color.ui auto
```

### 有用的别名

```bash
# 添加到 ~/.gitconfig
[alias]
    co = checkout
    br = branch
    ci = commit
    st = status
    unstage = reset HEAD --
    last = log -1 HEAD
    visual = log --oneline --graph --all