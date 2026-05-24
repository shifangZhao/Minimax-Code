---
name: coding-standards
description: 跨项目编码约定基准，包括命名、可读性、不可变性和代码质量审查。使用详细的前端或后端技能获取特定于框架的模式。
origin: ECC
---

# 编码标准与最佳实践

适用于项目的跨项目编码约定基准。

此技能是共享基础，而非详细的框架 playbook。

- 使用 `frontend-patterns` 获取 React、状态、表单、渲染和 UI 架构。
- 使用 `backend-patterns` 或 `api-design` 获取 repository/service 层、端点设计、验证和服务器特定关注点。
- 当需要最短的可重用规则层而非完整的技能演练时，使用 `rules/common/coding-style.md`。

## 激活时机

- 启动新项目或模块
- 审查代码质量和可维护性
- 重构现有代码以遵循约定
- 强制执行命名、格式或结构一致性
- 设置 lint、格式化或类型检查规则
- 为新贡献者介绍编码约定

## 范围边界

为此技能激活的场景：
- 描述性命名
- 不可变性默认值
- 可读性、KISS、DRY 和 YAGNI 强制执行
- 错误处理期望和代码气味审查

不要将此技能作为主要来源用于：
- React 组合、hooks 或渲染模式
- 后端架构、API 设计或数据库分层
- 当更窄的 ECC 技能已存在时的特定于域的框架指导

## 代码质量原则

### 1. 可读性优先
- 代码阅读多于编写
- 清晰的变量和函数名
- 优先自文档化代码而非注释
- 一致的格式化

### 2. KISS（保持简单，笨蛋）
- 最简单的可行解决方案
- 避免过度工程
- 不做 premature optimization
- 易于理解 > 聪明的代码

### 3. DRY（不要重复自己）
- 将通用逻辑提取到函数中
- 创建可复用组件
- 跨模块共享工具
- 避免复制粘贴编程

### 4. YAGNI（你不需要它）
- 不在未来需要之前构建功能
- 避免投机性通用性
- 仅在需要时才添加复杂性
- 先简单，必要时重构

## TypeScript/JavaScript 标准

### 变量命名

```typescript
// 通过：好：描述性名称
const marketSearchQuery = 'election'
const isUserAuthenticated = true
const totalRevenue = 1000

// 失败：坏：不清晰的名称
const q = 'election'
const flag = true
const x = 1000
```

### 函数命名

```typescript
// 通过：好：动词-名词模式
async function fetchMarketData(marketId: string) { }
function calculateSimilarity(a: number[], b: number[]) { }
function isValidEmail(email: string): boolean { }

// 失败：坏：不清晰或仅名词
async function market(id: string) { }
function similarity(a, b) { }
function email(e) { }
```

### 不可变性模式（关键）

```typescript
// 通过：始终使用展开运算符
const updatedUser = {
  ...user,
  name: 'New Name'
}

const updatedArray = [...items, newItem]

// 失败：永不直接修改
user.name = 'New Name'  // 坏
items.push(newItem)     // 坏
```

### 错误处理

```typescript
// 通过：好：全面的错误处理
async function fetchData(url: string) {
  try {
    const response = await fetch(url)

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`)
    }

    return await response.json()
  } catch (error) {
    console.error('Fetch failed:', error)
    throw new Error('Failed to fetch data')
  }
}

// 失败：坏：无错误处理
async function fetchData(url) {
  const response = await fetch(url)
  return response.json()
}
```

### Async/Await 最佳实践

```typescript
// 通过：好：尽可能并行执行
const [users, markets, stats] = await Promise.all([
  fetchUsers(),
  fetchMarkets(),
  fetchStats()
])

// 失败：坏：不必要时顺序执行
const users = await fetchUsers()
const markets = await fetchMarkets()
const stats = await fetchStats()
```

### 类型安全

```typescript
// 通过：好：适当的类型
interface Market {
  id: string
  name: string
  status: 'active' | 'resolved' | 'closed'
  created_at: Date
}

function getMarket(id: string): Promise<Market> {
  // Implementation
}

// 失败：坏：使用 'any'
function getMarket(id: any): Promise<any> {
  // Implementation
}
```

## React 最佳实践

### 组件结构

```typescript
// 通过：好：带有类型的函数组件
interface ButtonProps {
  children: React.ReactNode
  onClick: () => void
  disabled?: boolean
  variant?: 'primary' | 'secondary'
}

export function Button({
  children,
  onClick,
  disabled = false,
  variant = 'primary'
}: ButtonProps) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`btn btn-${variant}`}
    >
      {children}
    </button>
  )
}

// 失败：坏：无类型、结构不清晰
export function Button(props) {
  return <button onClick={props.onClick}>{props.children}</button>
}
```

### 自定义 Hooks

```typescript
// 通过：好：可复用自定义 hook
export function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value)

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value)
    }, delay)

    return () => clearTimeout(handler)
  }, [value, delay])

  return debouncedValue
}

// 使用
const debouncedQuery = useDebounce(searchQuery, 500)
```

### 状态管理

```typescript
// 通过：好：适当的状态更新
const [count, setCount] = useState(0)

// 基于先前状态的函数式更新
setCount(prev => prev + 1)

// 失败：坏：直接引用状态
setCount(count + 1)  // 在异步场景中可能是陈旧的
```

### 条件渲染

```typescript
// 通过：好：清晰的条件渲染
{isLoading && <Spinner />}
{error && <ErrorMessage error={error} />}

// 失败：坏：复杂的条件语句
{isLoading ? <Spinner /> : error ? <ErrorMessage /> : <Content />}
```