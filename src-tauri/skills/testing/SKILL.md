---
name: tdd-workflow
description: 编写新功能、修复 bug 或重构代码时使用此技能。强制测试驱动开发，覆盖率 80%+，包括单元、集成和 E2E 测试。
origin: ECC
---

# 测试驱动开发工作流

此技能确保所有代码开发遵循 TDD 原则和全面的测试覆盖率。

## 激活时机

- 编写新功能或特性
- 修复 bug 或问题
- 重构现有代码
- 添加 API 端点
- 创建新组件

## 核心原则

### 1. 测试优先于代码
始终先写测试，然后实现代码使测试通过。

### 2. 覆盖率要求
- 最低 80% 覆盖率（单元 + 集成 + E2E）
- 覆盖所有边界情况
- 测试错误场景
- 验证边界条件

### 3. 测试类型

#### 单元测试
- 独立函数和工具函数
- 组件逻辑
- 纯函数
- 辅助函数和工具函数

#### 集成测试
- API 端点
- 数据库操作
- 服务交互
- 外部 API 调用

#### E2E 测试（Playwright）
- 关键用户流程
- 完整工作流
- 浏览器自动化
- UI 交互

### 4. Git 检查点
- 如果仓库在 Git 下，在每个 TDD 阶段后创建检查点提交
- 在工作流完成前不要压缩或重写这些检查点提交
- 每个检查点提交消息必须描述阶段和捕获的确切证据
- 仅计算当前活动分支上当前任务的提交
- 不要将其他分支、更早无关工作或遥远分支历史的提交视为有效检查点证据
- 在将检查点视为满足之前，验证该提交可从活动分支的当前 `HEAD` 到达并且属于当前任务序列
- 首选紧凑工作流是：
  - 一个提交用于添加失败测试并验证 RED
  - 一个提交用于应用最小修复并验证 GREEN
  - 一个可选提交用于完成重构
- 如果测试提交明确对应 RED 且修复提交明确对应 GREEN，则不需要单独的证据提交

## TDD 工作流步骤

### 步骤 1：编写用户旅程
```
作为 [角色]，我希望 [动作]，以便 [收益]

示例：
作为用户，我希望语义搜索市场，
以便即使没有精确关键词也能找到相关市场。
```

### 步骤 2：生成测试用例
为每个用户旅程创建全面的测试用例：

```typescript
describe('Semantic Search', () => {
  it('returns relevant markets for query', async () => {
    // Test implementation
  })

  it('handles empty query gracefully', async () => {
    // Test edge case
  })

  it('falls back to substring search when Redis unavailable', async () => {
    // Test fallback behavior
  })

  it('sorts results by similarity score', async () => {
    // Test sorting logic
  })
})
```

### 步骤 3：运行测试（应该失败）
```bash
npm test
# 测试应该失败 — 我们还没有实现
```

此步骤是强制性的，是所有生产变更的 RED 门。

在修改业务逻辑或其他生产代码之前，你必须通过以下路径之一验证有效的 RED 状态：
- 运行时 RED：
  - 相关测试目标成功编译
  - 新或更改的测试实际被执行
  - 结果是 RED
- 编译时 RED：
  - 新测试新实例化、引用或行使有 bug 的代码路径
  - 编译失败本身就是预期的 RED 信号
- 无论哪种情况，失败是由预期的业务逻辑 bug、未定义行为或缺失实现引起的
- 失败不是仅由无关语法错误、破坏的测试设置、缺失依赖或无关回归引起的

仅编写但未编译和执行的测试不计入 RED。

在此 RED 状态确认之前不要编辑生产代码。

如果仓库在 Git 下，在此阶段验证后立即创建检查点提交。
建议的提交消息格式：
- `test: add reproducer for <feature or bug>`
- 如果复现器被编译和执行并且因预期原因失败，此提交也可以作为 RED 验证检查点
- 在继续之前验证此检查点提交在当前活动分支上

### 步骤 4：实现代码
写最小代码使测试通过：

```typescript
// Implementation guided by tests
export async function searchMarkets(query: string) {
  // Implementation here
}
```

如果仓库在 Git 下，现在暂存最小修复但推迟检查点提交直到步骤 5 中 GREEN 得到验证。

### 步骤 5：再次运行测试
```bash
npm test
# Tests should now pass
```

修复后重新运行相同的相关测试目标，并确认之前失败的测试现在为 GREEN。

仅在有效的 GREEN 结果后才能继续重构。

如果仓库在 Git 下，在 GREEN 验证后立即创建检查点提交。
建议的提交消息格式：
- `fix: <feature or bug>`
- 如果重新运行相同的相关测试目标并通过，修复提交也可以作为 GREEN 验证检查点
- 在继续之前验证此检查点提交在当前活动分支上

### 步骤 6：重构
在保持测试绿色时改进代码质量：
- 消除重复
- 改进命名
- 优化性能
- 增强可读性

如果仓库在 Git 下，在重构完成且测试保持绿色后立即创建检查点提交。
建议的提交消息格式：
- `refactor: clean up after <feature or bug> implementation`
- 在考虑 TDD 循环完成之前验证此检查点提交在当前活动分支上

### 步骤 7：验证覆盖率
```bash
npm run test:coverage
# Verify 80%+ coverage achieved
```

## 测试模式

### 单元测试模式（Jest/Vitest）
```typescript
import { render, screen, fireEvent } from '@testing-library/react'
import { Button } from './Button'

describe('Button Component', () => {
  it('renders with correct text', () => {
    render(<Button>Click me</Button>)
    expect(screen.getByText('Click me')).toBeInTheDocument()
  })

  it('calls onClick when clicked', () => {
    const handleClick = jest.fn()
    render(<Button onClick={handleClick}>Click</Button>)

    fireEvent.click(screen.getByRole('button'))

    expect(handleClick).toHaveBeenCalledTimes(1)
  })

  it('is disabled when disabled prop is true', () => {
    render(<Button disabled>Click</Button>)
    expect(screen.getByRole('button')).toBeDisabled()
  })
})
```

### API 集成测试模式
```typescript
import { NextRequest } from 'next/server'
import { GET } from './route'

describe('GET /api/markets', () => {
  it('returns markets successfully', async () => {
    const request = new NextRequest('http://localhost/api/markets')
    const response = await GET(request)
    const data = await response.json()

    expect(response.status).toBe(200)
    expect(data.success).toBe(true)
    expect(Array.isArray(data.data)).toBe(true)
  })

  it('validates query parameters', async () => {
    const request = new NextRequest('http://localhost/api/markets?limit=invalid')
    const response = await GET(request)

    expect(response.status).toBe(400)
  })

  it('handles database errors gracefully', async () => {
    // Mock database failure
    const request = new NextRequest('http://localhost/api/markets')
    // Test error handling
  })
})
```

### E2E 测试模式（Playwright）
```typescript
import { test, expect } from '@playwright/test'

test('user can search and filter markets', async ({ page }) => {
  // Navigate to markets page
  await page.goto('/')
  await page.click('a[href="/markets"]')

  // Verify page loaded
  await expect(page.locator('h1')).toContainText('Markets')

  // Search for markets
  await page.fill('input[placeholder="Search markets"]', 'election')

  // Wait for debounce and results
  await page.waitForTimeout(600)

  // Verify search results displayed
  const results = page.locator('[data-testid="market-card"]')
  await expect(results).toHaveCount(5, { timeout: 5000 })

  // Verify results contain search term
  const firstResult = results.first()
  await expect(firstResult).toContainText('election', { ignoreCase: true })

  // Filter by status
  await page.click('button:has-text("Active")')

  // Verify filtered results
  await expect(results).toHaveCount(3)
})

test('user can create a new market', async ({ page }) => {
  // Login first
  await page.goto('/creator-dashboard')

  // Fill market creation form
  await page.fill('input[name="name"]', 'Test Market')
  await page.fill('textarea[name="description"]', 'Test description')
  await page.fill('input[name="endDate"]', '2025-12-31')

  // Submit form
  await page.click('button[type="submit"]')

  // Verify success message
  await expect(page.locator('text=Market created successfully')).toBeVisible()

  // Verify redirect to market page
  await expect(page).toHaveURL(/\/markets\/test-market/)
})
```

## 测试文件组织

```
src/
├── components/
│   ├── Button/
│   │   ├── Button.tsx
│   │   ├── Button.test.tsx          # 单元测试
│   │   └── Button.stories.tsx       # Storybook
│   └── MarketCard/
│       ├── MarketCard.tsx
│       └── MarketCard.test.tsx
├── app/
│   └── api/
│       └── markets/
│           ├── route.ts
│           └── route.test.ts         # 集成测试
└── e2e/
    ├── markets.spec.ts               # E2E 测试
    ├── trading.spec.ts
    └── auth.spec.ts
```

## Mock 外部服务

### Supabase Mock
```typescript
jest.mock('@/lib/supabase', () => ({
  supabase: {
    from: jest.fn(() => ({
      select: jest.fn(() => ({
        eq: jest.fn(() => Promise.resolve({
          data: [{ id: 1, name: 'Test Market' }],
          error: null
        }))
      }))
    }))
  }
}))
```

### Redis Mock
```typescript
jest.mock('@/lib/redis', () => ({
  searchMarketsByVector: jest.fn(() => Promise.resolve([
    { slug: 'test-market', similarity_score: 0.95 }
  ])),
  checkRedisHealth: jest.fn(() => Promise.resolve({ connected: true }))
}))
```

### OpenAI Mock
```typescript
jest.mock('@/lib/openai', () => ({
  generateEmbedding: jest.fn(() => Promise.resolve(
    new Array(1536).fill(0.1) // Mock 1536-dim embedding
  ))
}))
```

## 测试覆盖率验证

### 运行覆盖率报告
```bash
npm run test:coverage
```

### 覆盖率阈值
```json
{
  "jest": {
    "coverageThresholds": {
      "global": {
        "branches": 80,
        "functions": 80,
        "lines": 80,
        "statements": 80
      }
    }
  }
}
```

## 应避免的常见测试错误

### 错误：测试实现细节
```typescript
// 不要测试内部状态
expect(component.state.count).toBe(5)
```

### 正确：测试用户可见行为
```typescript
// Test what users see
expect(screen.getByText('Count: 5')).toBeInTheDocument()
```

### 错误：脆弱的选择器
```typescript
// 容易破坏
await page.click('.css-class-xyz')
```

### 正确：语义选择器
```typescript
// 有弹性
await page.click('button:has-text("Submit")')
await page.click('[data-testid="submit-button"]')
```

### 错误：无测试隔离
```typescript
// 测试相互依赖
test('creates user', () => { /* ... */ })
test('updates same user', () => { /* depends on previous test */ })
```

### 正确：独立测试
```typescript
// 每个测试设置自己的数据
test('creates user', () => {
  const user = createTestUser()
  // Test logic
})

test('updates user', () => {
  const user = createTestUser()
  // Update logic
})
```

## 持续测试

### 开发期间监听模式
```bash
npm test -- --watch
# Tests run automatically on file changes
```

### 预提交钩子
```bash
# 每次提交前运行
npm test && npm run lint
```

### CI/CD 集成
```yaml
# GitHub Actions
- name: Run Tests
  run: npm test -- --coverage
- name: Upload Coverage
  uses: codecov/codecov-action@v3
```

## 最佳实践

1. **先写测试** - 始终 TDD
2. **每个测试一个断言** - 专注于单一行为
3. **描述性测试名称** - 解释测试了什么
4. **Arrange-Act-Assert** - 清晰的测试结构
5. **Mock 外部依赖** - 隔离单元测试
6. **测试边界情况** - Null、undefined、空、大
7. **测试错误路径** - 不只是 happy paths
8. **保持测试快速** - 单元测试 < 50ms 每个
9. **测试后清理** - 无副作用
10. **审查覆盖率报告** - 识别差距

## 成功指标

- 实现 80%+ 代码覆盖率
- 所有测试通过（绿色）
- 无跳过或禁用的测试
- 快速测试执行（单元测试 < 30s）
- E2E 测试覆盖关键用户流程
- 测试在生产前捕获 bug

---

**记住**：测试不是可选的。它们是使自信重构、快速开发和生产可靠性成为可能的安全网。

---

---
name: e2e-testing
description: Playwright E2E 测试模式、页面对象模型、配置、CI/CD 集成、制品管理和 flaky 测试策略。
origin: ECC
---

# E2E 测试模式

用于构建稳定、快速和可维护 E2E 测试套件的全面 Playwright 模式。

## 测试文件组织

```
tests/
├── e2e/
│   ├── auth/
│   │   ├── login.spec.ts
│   │   ├── logout.spec.ts
│   │   └── register.spec.ts
│   ├── features/
│   │   ├── browse.spec.ts
│   │   ├── search.spec.ts
│   │   └── create.spec.ts
│   └── api/
│       └── endpoints.spec.ts
├── fixtures/
│   ├── auth.ts
│   └── data.ts
└── playwright.config.ts
```

## 页面对象模型（POM）

### POM 结构
```typescript
import { Page, Locator } from '@playwright/test'

export class ItemsPage {
  readonly page: Page
  readonly searchInput: Locator
  readonly itemCards: Locator
  readonly createButton: Locator

  constructor(page: Page) {
    this.page = page
    this.searchInput = page.locator('[data-testid="search-input"]')
    this.itemCards = page.locator('[data-testid="item-card"]')
    this.createButton = page.locator('[data-testid="create-btn"]')
  }

  async goto() {
    await this.page.goto('/items')
    await this.page.waitForLoadState('networkidle')
  }

  async search(query: string) {
    await this.searchInput.fill(query)
    await this.page.waitForResponse(resp => resp.url().includes('/api/search'))
    await this.page.waitForLoadState('networkidle')
  }

  async getItemCount() {
    return await this.itemCards.count()
  }
}
```

## 测试结构

```typescript
import { test, expect } from '@playwright/test'
import { ItemsPage } from '../../pages/ItemsPage'

test.describe('Item Search', () => {
  let itemsPage: ItemsPage

  test.beforeEach(async ({ page }) => {
    itemsPage = new ItemsPage(page)
    await itemsPage.goto()
  })

  test('should search by keyword', async ({ page }) => {
    await itemsPage.search('test')

    const count = await itemsPage.getItemCount()
    expect(count).toBeGreaterThan(0)

    await expect(itemsPage.itemCards.first()).toContainText(/test/i)
    await page.screenshot({ path: 'artifacts/search-results.png' })
  })

  test('should handle no results', async ({ page }) => {
    await itemsPage.search('xyznonexistent123')

    await expect(page.locator('[data-testid="no-results"]')).toBeVisible()
    expect(await itemsPage.getItemCount()).toBe(0)
  })
})
```

## Playwright 配置

```typescript
import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: [
    ['html', { outputFolder: 'playwright-report' }],
    ['junit', { outputFile: 'playwright-results.xml' }],
    ['json', { outputFile: 'playwright-results.json' }]
  ],
  use: {
    baseURL: process.env.BASE_URL || 'http://localhost:3000',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    actionTimeout: 10000,
    navigationTimeout: 30000,
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
    { name: 'mobile-chrome', use: { ...devices['Pixel 5'] } },
  ],
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:3000',
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
})
```

## Flaky 测试模式

### 隔离

```typescript
test('flaky: complex search', async ({ page }) => {
  test.fixme(true, 'Flaky - Issue #123')
  // test code...
})

test('conditional skip', async ({ page }) => {
  test.skip(process.env.CI, 'Flaky in CI - Issue #123')
  // test code...
})
```

### 识别 Flakiness

```bash
npx playwright test tests/search.spec.ts --repeat-each=10
npx playwright test tests/search.spec.ts --retries=3
```

### 常见原因和修复

**竞争条件：**
```typescript
// 坏：假设元素已就绪
await page.click('[data-testid="button"]')

// 好：自动等待定位器
await page.locator('[data-testid="button"]').click()
```

**网络时序：**
```typescript
// 坏：任意超时
await page.waitForTimeout(5000)

// 好：等待特定条件
await page.waitForResponse(resp => resp.url().includes('/api/data'))
```

**动画时序：**
```typescript
// 坏：在动画期间点击
await page.click('[data-testid="menu-item"]')

// 好：等待稳定
await page.locator('[data-testid="menu-item"]').waitFor({ state: 'visible' })
await page.waitForLoadState('networkidle')
await page.locator('[data-testid="menu-item"]').click()
```

## 制品管理

### 截图

```typescript
await page.screenshot({ path: 'artifacts/after-login.png' })
await page.screenshot({ path: 'artifacts/full-page.png', fullPage: true })
await page.locator('[data-testid="chart"]').screenshot({ path: 'artifacts/chart.png' })
```

### 追踪

```typescript
await browser.startTracing(page, {
  path: 'artifacts/trace.json',
  screenshots: true,
  snapshots: true,
})
// ... test actions ...
await browser.stopTracing()
```

### 视频

```typescript
// In playwright.config.ts
use: {
  video: 'retain-on-failure',
  videosPath: 'artifacts/videos/'
}
```

## CI/CD 集成

```yaml
# .github/workflows/e2e.yml
name: E2E Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - run: npm ci
      - run: npx playwright install --with-deps
      - run: npx playwright test
        env:
          BASE_URL: ${{ vars.STAGING_URL }}
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: playwright-report
          path: playwright-report/
          retention-days: 30
```

## 测试报告模板

```markdown
# E2E 测试报告

**日期：** YYYY-MM-DD HH:MM
**持续时间：** Xm Ys
**状态：** 通过 / 失败

## 摘要
- 总计：X | 通过：Y (Z%) | 失败：A | Flaky：B | 跳过：C

## 失败的测试

### test-name
**文件：** `tests/e2e/feature.spec.ts:45`
**错误：** 期望元素可见
**截图：** artifacts/failed.png
**推荐修复：** [description]

## 制品
- HTML 报告：playwright-report/index.html
- 截图：artifacts/*.png
- 视频：artifacts/videos/*.webm
- 追踪：artifacts/*.zip
```

## 钱包/Web3 测试

```typescript
test('wallet connection', async ({ page, context }) => {
  // Mock wallet provider
  await context.addInitScript(() => {
    window.ethereum = {
      isMetaMask: true,
      request: async ({ method }) => {
        if (method === 'eth_requestAccounts')
          return ['0x1234567890123456789012345678901234567890']
        if (method === 'eth_chainId') return '0x1'
      }
    }
  })

  await page.goto('/')
  await page.locator('[data-testid="connect-wallet"]').click()
  await expect(page.locator('[data-testid="wallet-address"]')).toContainText('0x1234')
})
```

## 金融/关键流程测试

```typescript
test('trade execution', async ({ page }) => {
  // Skip on production — real money
  test.skip(process.env.NODE_ENV === 'production', 'Skip on production')

  await page.goto('/markets/test-market')
  await page.locator('[data-testid="position-yes"]').click()
  await page.locator('[data-testid="trade-amount"]').fill('1.0')

  // Verify preview
  const preview = page.locator('[data-testid="trade-preview"]')
  await expect(preview).toContainText('1.0')

  // Confirm and wait for blockchain
  await page.locator('[data-testid="confirm-trade"]').click()
  await page.waitForResponse(
    resp => resp.url().includes('/api/trade') && resp.status() === 200,
    { timeout: 30000 }
  )

  await expect(page.locator('[data-testid="trade-success"]')).toBeVisible()
})
```

---

---
name: ai-regression-testing
description: AI 辅助开发的回归测试策略。沙箱模式 API 测试无数据库依赖，自动化 bug 检查工作流，以及捕获 AI 盲点的模式——同一模型编写和审查代码。
origin: ECC
---

# AI 回归测试

专门为 AI 辅助开发设计的测试模式，其中同一模型编写代码并审查它 — 创建仅自动化测试才能捕获的系统性盲点。

## 激活时机

- AI agent（Claude Code、Cursor、Codex）修改了 API 路由或后端逻辑
- 发现了 bug 并修复 — 需要防止重新引入
- 项目有可利用的沙箱/mock 模式用于 DB-free 测试
- 在代码更改后运行 `/bug-check` 或类似审查命令
- 存在多条代码路径（沙箱 vs 生产、功能标志等）

## 核心问题

当 AI 编写代码然后审查自己的工作时，它将相同的假设带入两个步骤。这创建了可预测的失败模式：

```
AI 写修复 → AI 审查修复 → AI 说"看起来正确" → Bug 仍然存在
```

**真实世界示例**（在生产中观察到）：

```
修复 1：添加 notification_settings 到 API 响应
  → 忘记添加到 SELECT 查询
  → AI 审查并遗漏（相同的盲点）

修复 2：添加到 SELECT 查询
  → TypeScript 构建错误（列不在生成的类型中）
  → AI 审查修复 1 但没捕获 SELECT 问题

修复 3：改为 SELECT *
  → 修复生产路径，忘记沙箱路径
  → AI 审查并再次遗漏（第四次发生）

修复 4：测试在首次运行时立即捕获
  PASS:
```

模式：**沙箱/生产路径不一致** 是 #1 AI 引入的回归。

## 沙箱模式 API 测试

大多数具有 AI 友好架构的项目都有沙箱/mock 模式。这是快速、DB-free API 测试的关键。

### 设置（Vitest + Next.js App Router）

```typescript
// vitest.config.ts
import { defineConfig } from "vitest/config";
import path from "path";

export default defineConfig({
  test: {
    environment: "node",
    globals: true,
    include: ["__tests__/**/*.test.ts"],
    setupFiles: ["__tests__/setup.ts"],
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "."),
    },
  },
});
```

```typescript
// __tests__/setup.ts
// 强制沙箱模式 — 无需数据库
process.env.SANDBOX_MODE = "true";
process.env.NEXT_PUBLIC_SUPABASE_URL = "";
process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY = "";
```

### Next.js API 路由的测试辅助函数

```typescript
// __tests__/helpers.ts
import { NextRequest } from "next/server";

export function createTestRequest(
  url: string,
  options?: {
    method?: string;
    body?: Record<string, unknown>;
    headers?: Record<string, string>;
    sandboxUserId?: string;
  },
): NextRequest {
  const { method = "GET", body, headers = {}, sandboxUserId } = options || {};
  const fullUrl = url.startsWith("http") ? url : `http://localhost:3000${url}`;
  const reqHeaders: Record<string, string> = { ...headers };

  if (sandboxUserId) {
    reqHeaders["x-sandbox-user-id"] = sandboxUserId;
  }

  const init: { method: string; headers: Record<string, string>; body?: string } = {
    method,
    headers: reqHeaders,
  };

  if (body) {
    init.body = JSON.stringify(body);
    reqHeaders["content-type"] = "application/json";
  }

  return new NextRequest(fullUrl, init);
}

export async function parseResponse(response: Response) {
  const json = await response.json();
  return { status: response.status, json };
}
```

### 编写回归测试

关键原则：**为发现的 bug 编写测试，不为正常工作的代码编写测试**。

```typescript
// __tests__/api/user/profile.test.ts
import { describe, it, expect } from "vitest";
import { createTestRequest, parseResponse } from "../../helpers";
import { GET, PATCH } from "@/app/api/user/profile/route";

// 定义契约 — 响应中必须有哪些字段
const REQUIRED_FIELDS = [
  "id",
  "email",
  "full_name",
  "phone",
  "role",
  "created_at",
  "avatar_url",
  "notification_settings",  // ← 发现 bug 缺失后添加
];

describe("GET /api/user/profile", () => {
  it("returns all required fields", async () => {
    const req = createTestRequest("/api/user/profile");
    const res = await GET(req);
    const { status, json } = await parseResponse(res);

    expect(status).toBe(200);
    for (const field of REQUIRED_FIELDS) {
      expect(json.data).toHaveProperty(field);
    }
  });

  // 回归测试 — 这个确切的 bug 被 AI 引入了 4 次
  it("notification_settings is not undefined (BUG-R1 regression)", async () => {
    const req = createTestRequest("/api/user/profile");
    const res = await GET(req);
    const { json } = await parseResponse(res);

    expect("notification_settings" in json.data).toBe(true);
    const ns = json.data.notification_settings;
    expect(ns === null || typeof ns === "object").toBe(true);
  });
});
```

### 测试沙箱/生产奇偶校验

最常见的 AI 回归：修复生产路径但忘记沙箱路径（反之亦然）。

```typescript
// 测试沙箱响应与预期契约匹配
describe("GET /api/user/messages (conversation list)", () => {
  it("includes partner_name in sandbox mode", async () => {
    const req = createTestRequest("/api/user/messages", {
      sandboxUserId: "user-001",
    });
    const res = await GET(req);
    const { json } = await parseResponse(res);

    // 这捕获了一个 bug，其中 partner_name 被添加到
    // 生产路径但未添加到沙箱路径
    if (json.data.length > 0) {
      for (const conv of json.data) {
        expect("partner_name" in conv).toBe(true);
      }
    }
  });
});
```

## 将测试集成到 Bug-Check 工作流

### 自定义命令定义

```markdown
<!-- .claude/commands/bug-check.md -->
# Bug Check

## 步骤 1：自动化测试（强制性，不能跳过）

首先运行这些命令，然后再进行任何代码审查：

    npm run test       # Vitest 测试套件
    npm run build      # TypeScript 类型检查 + 构建

- 如果测试失败 → 报告为最高优先级 bug
- 如果构建失败 → 报告类型错误为最高优先级
- 仅在两者都通过时才继续步骤 2

## 步骤 2：代码审查（AI 审查）

1. 沙箱/生产路径一致性
2. API 响应形状匹配前端期望
3. SELECT 子句完整性
4. 带回滚的错误处理
5. 乐观更新竞争条件

## 步骤 3：对于每个修复的 bug，提出回归测试
```

### 工作流

```
用户："バグチェックして"（或 "/bug-check")
  │
  ├─ 步骤 1：npm run test
  │   ├─ FAIL → 机械地发现 Bug（不需要 AI 判断）
  │   └─ PASS → 继续
  │
  ├─ 步骤 2：npm run build
  │   ├─ FAIL → 机械地发现类型错误
  │   └─ PASS → 继续
  │
  ├─ 步骤 3：AI 代码审查（知道已知盲点）
  │   └─ 发现报告
  │
  └─ 步骤 4：对于每个修复，编写回归测试
      └─ 下一个 bug-check 捕获修复是否破坏
```

## 常见 AI 回归模式

### 模式 1：沙箱/生产路径不匹配

**频率**：最常见（在 4 次回归中观察到 3 次）

```typescript
// 失败：AI 仅添加到生产路径
if (isSandboxMode()) {
  return { data: { id, email, name } };  // 缺少新字段
}
// 生产路径
return { data: { id, email, name, notification_settings } };

// 通过：两个路径必须返回相同形状
if (isSandboxMode()) {
  return { data: { id, email, name, notification_settings: null } };
}
return { data: { id, email, name, notification_settings } };
```

**测试捕获它**：

```typescript
it("sandbox and production return same fields", async () => {
  // In test env, sandbox mode is forced ON
  const res = await GET(createTestRequest("/api/user/profile"));
  const { json } = await parseResponse(res);

  for (const field of REQUIRED_FIELDS) {
    expect(json.data).toHaveProperty(field);
  }
});
```

### 模式 2：SELECT 子句遗漏

**频率**：使用 Supabase/Prisma 添加新列时常见

```typescript
// 失败：新列添加到响应但未添加到 SELECT
const { data } = await supabase
  .from("users")
  .select("id, email, name")  // notification_settings 不在这里
  .single();

return { data: { ...data, notification_settings: data.notification_settings } };
// → notification_settings 永远是 undefined

// 通过：使用 SELECT * 或显式包含新列
const { data } = await supabase
  .from("users")
  .select("*")
  .single();
```

### 模式 3：错误状态泄漏

**频率**：中等 — 添加错误处理到现有组件时

```typescript
// 失败：设置错误状态但未清除旧数据
catch (err) {
  setError("Failed to load");
  // reservations 仍显示来自上一个选项卡的数据！
}

// 通过：错误时清除相关状态
catch (err) {
  setReservations([]);  // 清除陈旧数据
  setError("Failed to load");
}
```

### 模式 4：没有适当回滚的乐观更新

```typescript
// 失败：失败时无回滚
const handleRemove = async (id: string) => {
  setItems(prev => prev.filter(i => i.id !== id));
  await fetch(`/api/items/${id}`, { method: "DELETE" });
  // 如果 API 失败，item 从 UI 消失但仍在 DB 中
};

// 通过：捕获先前状态并在失败时回滚
const handleRemove = async (id: string) => {
  const prevItems = [...items];
  setItems(prev => prev.filter(i => i.id !== id));
  try {
    const res = await fetch(`/api/items/${id}`, { method: "DELETE" });
    if (!res.ok) throw new Error("API error");
  } catch {
    setItems(prevItems);  // 回滚
    alert("削除に失敗しました");
  }
};
```

## 策略：在发现 Bug 的地方测试

不要以 100% 覆盖率为目标。反而：

```
在 /api/user/profile 发现 bug     → 为 profile API 编写测试
在 /api/user/messages 发现 bug    → 为 messages API 编写测试
在 /api/user/favorites 发现 bug   → 为 favorites API 编写测试
在 /api/user/notifications 没 bug → 不编写测试（还不需要）
```

**为什么这对 AI 开发有效：**

1. AI 倾向于重复犯**相同类别的错误**
2. Bug 聚集在复杂区域（auth、多路径逻辑、状态管理）
3. 一旦测试，该确切回归**不能再发生**
4. 测试计数随 bug 修复有机增长 — 无浪费努力

## 快速参考

| AI 回归模式 | 测试策略 | 优先级 |
|---|---|---|
| 沙箱/生产不匹配 | 在沙箱模式下断言相同响应形状 | 高 |
| SELECT 子句遗漏 | 断言响应中所有必需字段 | 高 |
| 错误状态泄漏 | 断言错误时状态清理 | 中 |
| 缺失回滚 | 断言 API 失败时状态恢复 | 中 |
| 类型转换掩盖 null | 断言字段不是 undefined | 中 |

## 做 / 不做

**做：**
- 在发现 bug 后立即编写测试（如果可能，在修复之前）
- 测试 API 响应形状，而非实现
- 将测试作为每个 bug-check 的第一步运行
- 保持测试快速（使用沙箱模式总计 < 1 秒）
- 用它们防止的 bug 命名测试（例如，"BUG-R1 regression"）

**不要：**
- 为从未有 bug 的代码编写测试
- 将 AI 自我审查作为自动化测试的替代品信任
- 因为"只是 mock 数据"而跳过沙箱路径测试
- 当单元测试足够时编写集成测试
- 以覆盖率百分比为目标 — 以回归预防为目标