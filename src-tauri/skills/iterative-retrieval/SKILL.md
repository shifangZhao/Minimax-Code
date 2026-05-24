---
name: iterative-retrieval
description: 逐步细化上下文检索以解决子智能体上下文问题的模式
origin: ECC
---

# 迭代检索模式

解决多智能体工作流中的"上下文问题"，即子智能体在开始工作前不知道需要什么上下文。

## 激活时机

- 生成需要无法预测上下文的子智能体
- 构建上下文逐步细化的多智能体工作流
- 遇到智能体任务中的"上下文太大"或"缺少上下文"失败
- 设计类似 RAG 的代码探索检索管道
- 优化智能体编排中的 token 使用

## 问题

子智能体以有限上下文生成。他们不知道：
- 哪些文件包含相关代码
- 代码库中存在什么模式
- 项目使用什么术语

标准方法失败：
- **发送一切**：超出上下文限制
- **发送无**：智能体缺乏关键信息
- **猜测需要什么**：经常错误

## 解决方案：迭代检索

一个 4 阶段循环，逐步细化上下文：

```
┌─────────────────────────────────────────────┐
│                                             │
│   ┌──────────┐      ┌──────────┐            │
│   │ DISPATCH │─────│ EVALUATE │            │
│   └──────────┘      └──────────┘            │
│        ▲                  │                 │
│        │                  ▼                 │
│   ┌──────────┐      ┌──────────┐            │
│   │   LOOP   │─────│  REFINE  │            │
│   └──────────┘      └──────────┘            │
│                                             │
│        最多 3 个循环，然后继续               │
└─────────────────────────────────────────────┘
```

### 阶段 1：DISPATCH

初始广泛查询以收集候选文件：

```javascript
// 从高层意图开始
const initialQuery = {
  patterns: ['src/**/*.ts', 'lib/**/*.ts'],
  keywords: ['authentication', 'user', 'session'],
  excludes: ['*.test.ts', '*.spec.ts']
};

// 分发到检索智能体
const candidates = await retrieveFiles(initialQuery);
```

### 阶段 2：EVALUATE

评估检索内容的相关性：

```javascript
function evaluateRelevance(files, task) {
  return files.map(file => ({
    path: file.path,
    relevance: scoreRelevance(file.content, task),
    reason: explainRelevance(file.content, task),
    missingContext: identifyGaps(file.content, task)
  }));
}
```

评分标准：
- **高（0.8-1.0）**：直接实现目标功能
- **中（0.5-0.7）**：包含相关模式或类型
- **低（0.2-0.4）**：略微相关
- **无（0-0.2）**：不相关，排除

### 阶段 3：REFINE

基于评估更新搜索条件：

```javascript
function refineQuery(evaluation, previousQuery) {
  return {
    // 从高相关性文件中添加新发现的模式
    patterns: [...previousQuery.patterns, ...extractPatterns(evaluation)],

    // 添加代码库中找到的术语
    keywords: [...previousQuery.keywords, ...extractKeywords(evaluation)],

    // 排除确认不相关的路径
    excludes: [...previousQuery.excludes, ...evaluation
      .filter(e => e.relevance < 0.2)
      .map(e => e.path)
    ],

    // 关注特定缺口
    focusAreas: evaluation
      .flatMap(e => e.missingContext)
      .filter(unique)
  };
}
```

### 阶段 4：LOOP

用细化条件重复（最多 3 个循环）：

```javascript
async function iterativeRetrieve(task, maxCycles = 3) {
  let query = createInitialQuery(task);
  let bestContext = [];

  for (let cycle = 0; cycle < maxCycles; cycle++) {
    const candidates = await retrieveFiles(query);
    const evaluation = evaluateRelevance(candidates, task);

    // 检查是否有足够上下文
    const highRelevance = evaluation.filter(e => e.relevance >= 0.7);
    if (highRelevance.length >= 3 && !hasCriticalGaps(evaluation)) {
      return highRelevance;
    }

    // 细化并继续
    query = refineQuery(evaluation, query);
    bestContext = mergeContext(bestContext, highRelevance);
  }

  return bestContext;
}
```

## 实践示例

### 示例 1：Bug 修复上下文

```
任务："修复认证 token 过期 bug"

循环 1：
  DISPATCH：在 src/** 中搜索 "token"、"auth"、"expiry"
  EVALUATE：找到 auth.ts (0.9)、tokens.ts (0.8)、user.ts (0.3)
  REFINE：添加 "refresh"、"jwt" 关键词；排除 user.ts

循环 2：
  DISPATCH：搜索细化术语
  EVALUATE：找到 session-manager.ts (0.95)、jwt-utils.ts (0.85)
  REFINE：有足够上下文（2 个高相关性文件）

结果：auth.ts、tokens.ts、session-manager.ts、jwt-utils.ts
```

### 示例 2：功能实现

```
任务："为 API 端点添加速率限制"

循环 1：
  DISPATCH：在 routes/** 中搜索 "rate"、"limit"、"api"
  EVALUATE：无匹配 — 代码库使用 "throttle" 术语
  REFINE：添加 "throttle"、"middleware" 关键词

循环 2：
  DISPATCH：搜索细化术语
  EVALUATE：找到 throttle.ts (0.9)、middleware/index.ts (0.7)
  REFINE：需要路由器模式

循环 3：
  DISPATCH：搜索 "router"、"express" 模式
  EVALUATE：找到 router-setup.ts (0.8)
  REFINE：有足够上下文

结果：throttle.ts、middleware/index.ts、router-setup.ts
```

## 与智能体集成

在智能体提示中使用：

```markdown
为此任务检索上下文时：
1. 从广泛关键词搜索开始
2. 评估每个文件的相关性（0-1 量表）
3. 识别仍缺少什么上下文
4. 细化搜索条件并重复（最多 3 个循环）
5. 返回相关性 >= 0.7 的文件
```

## 最佳实践

1. **从广泛开始，逐步缩小** - 不要过度指定初始查询
2. **学习代码库术语** - 第一循环经常揭示命名约定
3. **跟踪缺失内容** - 明确缺口识别推动细化
4. **在"足够好"时停止** - 3 个高相关性文件胜过 10 个中等文件
5. **自信地排除** - 低相关性文件不会变得相关

## 相关

- [Longform Guide](https://x.com/affaanmustafa/status/2014040193557471352) - 子智能体编排部分
- `continuous-learning` 技能 - 用于随时间改进的模式
- 与 ECC 捆绑的智能体定义（手动安装路径：`agents/`）