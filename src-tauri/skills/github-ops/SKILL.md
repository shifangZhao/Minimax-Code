---
name: github-ops
description: GitHub 仓库操作、自动化和管理。使用 gh CLI 进行问题分类、PR 管理、CI/CD 操作、发布管理和安全监控。在用户想要管理 GitHub issues、PR、CI 状态、发布、贡献者、陈旧项目或任何超出简单 git 命令的 GitHub 操作任务时使用。
origin: ECC
---

# GitHub 操作

管理 GitHub 仓库，重点关注社区健康、CI 可靠性和贡献者体验。

## 激活时机

- 分类问题（分类、打标签、回复、去重）
- 管理 PR（审查状态、CI 检查、陈旧 PR、合并就绪）
- 调试 CI/CD 失败
- 准备发布和 changelog
- 监控 Dependabot 和安全警报
- 管理开源项目的贡献者体验
- 用户说"检查 GitHub"、"分类问题"、"审查 PR"、"合并"、"发布"、"CI 坏了"

## 工具要求

- **gh CLI** 用于所有 GitHub API 操作
- 通过 `gh auth login` 配置仓库访问

## 问题分类

按类型和优先级分类每个问题：

**类型：** bug、feature-request、question、documentation、enhancement、duplicate、invalid、good-first-issue

**优先级：** critical（破坏性/安全）、high（重大影响）、medium（锦上添花）、low（ Cosmetic）

### 分类工作流

1. 阅读问题标题、正文和评论
2. 检查是否重复现有问题（按关键词搜索）
3. 通过 `gh issue edit --add-label` 应用适当的标签
4. 对于问题：起草并发布有用的回复
5. 对于需要更多信息的 bug：请求复现步骤
6. 对于好的首发问题：添加 `good-first-issue` 标签
7. 对于重复：评论并链接到原始问题，添加 `duplicate` 标签

```bash
# 搜索潜在重复
gh issue list --search "keyword" --state all --limit 20

# 添加标签
gh issue edit <number> --add-label "bug,high-priority"

# 评论问题
gh issue comment <number> --body "Thanks for reporting. Could you share reproduction steps?"
```

## PR 管理

### 审查检查清单

1. 检查 CI 状态：`gh pr checks <number>`
2. 检查是否可合并：`gh pr view <number> --json mergeable`
3. 检查年龄和最近活动
4. 标记 5 天以上无审查的 PR
5. 对于社区 PR：确保有测试并遵循约定

### 陈旧策略

- 14 天以上无活动的问题：添加 `stale` 标签，评论请求更新
- 7 天以上无活动的 PR：评论询问是否仍活跃
- 30 天无响应后自动关闭陈旧问题（添加 `closed-stale` 标签）

```bash
# 查找陈旧问题（14 天以上无活动）
gh issue list --label "stale" --state open

# 查找最近无活动的 PR
gh pr list --json number,title,updatedAt --jq '.[] | select(.updatedAt < "2026-03-01")'
```

## CI/CD 操作

当 CI 失败时：

1. 检查工作流运行：`gh run view <run-id> --log-failed`
2. 识别失败的步骤
3. 检查是 flaky 测试还是真实失败
4. 对于真实失败：识别根本原因并建议修复
5. 对于 flaky 测试：记录模式以供后续调查

```bash
# 列出最近失败的运行
gh run list --status failure --limit 10

# 查看失败运行日志
gh run view <run-id> --log-failed

# 重新运行失败的工作流
gh run rerun <run-id> --failed
```

## 发布管理

准备发布时：

1. 检查 main 上所有 CI 都是绿色的
2. 审查未发布变更：`gh pr list --state merged --base main`
3. 从 PR 标题生成 changelog
4. 创建发布：`gh release create`

```bash
# 列出自上次发布以来合并的 PR
gh pr list --state merged --base main --search "merged:>2026-03-01"

# 创建发布
gh release create v1.2.0 --title "v1.2.0" --generate-notes

# 创建预发布
gh release create v1.3.0-rc1 --prerelease --title "v1.3.0 Release Candidate 1"
```

## 安全监控

```bash
# 检查 Dependabot 警报
gh api repos/{owner}/{repo}/dependabot/alerts --jq '.[].security_advisory.summary'

# 检查秘密扫描警报
gh api repos/{owner}/{repo}/secret-scanning/alerts --jq '.[].state'

# 审查并自动合并安全依赖更新
gh pr list --label "dependencies" --json number,title
```

- 审查并自动合并安全依赖更新
- 立即标记任何 critical/high 严重性警报
- 至少每周检查新的 Dependabot 警报

## 质量门

完成任何 GitHub 操作任务前：
- 所有分类的问题都有适当的标签
- 没有超过 7 天无审查或评论的 PR
- CI 失败已调查（不仅仅是重新运行）
- 发布包含准确的 changelog
- 安全警报已被确认并跟踪