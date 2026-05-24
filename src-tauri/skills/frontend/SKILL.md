---
name: frontend-design
description: 创建独特、生产级前端界面，具有高设计质量。当用户要求构建 web 组件、页面或应用程序且视觉效果与代码质量同等重要时使用。
origin: ECC
---

# 前端设计

当任务不仅是"让它工作"而是"让它看起来经过设计"时使用此技能。

此技能用于产品页面、仪表板、应用界面或需要清晰观点而非通用 AI 界面的视觉系统。

## 使用场景

- 从零开始构建登录页面、仪表板或应用界面
- 将平淡界面升级为有意图且令人难忘的界面
- 将产品概念转化为具体的视觉方向
- 实现前端，其中排版、构图和动效很重要

## 核心原则

选择一个方向并坚持它。

安全、普通的 UI 通常比具有几个大胆选择强烈、一致的美学更差。

## 设计工作流

### 1. 首先构建设计框架

编码前先确定：

- 目的
- 受众
- 情感基调
- 视觉方向
- 用户应该记住的一件事

可能的方向：

- 残酷极简
- 编辑风格
- 工业风
- 奢侈感
- 活泼
- 几何
- 复古未来主义
- 柔和有机
- 最大主义

不要随意混合方向。选择一个并干净地执行。

### 2. 构建视觉系统

定义：

- 类型层次
- 颜色变量
- 间距节奏
- 布局逻辑
- 动效规则
- 表面/边框/阴影处理

使用 CSS 变量或项目的 token 系统，以便界面随增长保持一致。

### 3. 有意图地组合

优先：

- 当它强化层次时的不对称
- 当它创造深度时的重叠
- 当它澄清焦点时的强留白
- 仅当产品受益于密度时的密集布局

除非明确合适，否则不要默认使用对称卡片网格。

### 4. 使动效有意义

使用动画来：

- 揭示层次
- 分阶段展示信息
- 强化用户操作
- 创造一到两个难忘时刻

不要到处散布通用的微交互。一个精心设计的加载序列通常比二十个随机悬停效果更强。

## 强默认设置

### 排版

- 选择有特色的字体
- 在适当时配对独特的展示字体和易读的正文字体
- 当页面是设计主导时避免通用默认值

### 颜色

- 承诺清晰的调色板
- 一个占主导地位的色场配选择性强调通常比均匀加权的彩虹调色板效果更好
- 避免陈词滥调的紫-白渐变，除非产品真正需要

### 背景

使用氛围：

- 渐变
- 网格
- 纹理
- 细微噪点
- 图案
- 分层透明

平坦空背景很少是面向产品页面的最佳答案。

### 布局

- 当构图受益时打破网格
- 有意图地使用对角线、偏移和分组
- 即使布局非传统也要保持阅读流清晰

## 反模式

绝不默认：

- 可互换的 SaaS 英雄区域
- 没有层次感的通用卡片堆
- 没有系统的随机强调色
- 看起来像占位符的排版
- 仅因为添加动画容易而存在的动效

## 执行规则

- 在现有产品内工作时保留已建立的设计系统
- 使技术复杂度与视觉创意相匹配
- 保持可访问性和响应性完整
- 前端在桌面和移动端都应感觉经过深思熟虑

## 质量门

交付前：

- 界面有清晰的视觉观点
- 排版和间距感觉有意图
- 颜色和动效支持产品而非随机装饰
- 结果不像通用的 AI UI
- 实现是生产级的，不只是视觉上有趣

---

---
name: frontend-patterns
description: React、Next.js、状态管理、性能优化和 UI 最佳实践的前端开发模式。
origin: ECC
---

# 前端开发模式

React、Next.js 和高性能用户界面的现代前端模式。

## 激活时机

- 构建 React 组件（组合、props、渲染）
- 管理状态（useState、useReducer、Zustand、Context）
- 实现数据获取（SWR、React Query、server components）
- 优化性能（记忆化、虚拟化、代码分割）
- 处理表单（验证、受控输入、Zod schema）
- 处理客户端路由和导航
- 构建可访问、响应式 UI 模式

## 组件模式

### 组合优于继承

```typescript
// 通过：好的：组件组合
interface CardProps {
  children: React.ReactNode
  variant?: 'default' | 'outlined'
}

export function Card({ children, variant = 'default' }: CardProps) {
  return <div className={`card card-${variant}`}>{children}</div>
}

export function CardHeader({ children }: { children: React.ReactNode }) {
  return <div className="card-header">{children}</div>
}

export function CardBody({ children }: { children: React.ReactNode }) {
  return <div className="card-body">{children}</div>
}

// 使用
<Card>
  <CardHeader>标题</CardHeader>
  <CardBody>内容</CardBody>
</Card>
```

### 复合组件

```typescript
interface TabsContextValue {
  activeTab: string
  setActiveTab: (tab: string) => void
}

const TabsContext = createContext<TabsContextValue | undefined>(undefined)

export function Tabs({ children, defaultTab }: {
  children: React.ReactNode
  defaultTab: string
}) {
  const [activeTab, setActiveTab] = useState(defaultTab)

  return (
    <TabsContext.Provider value={{ activeTab, setActiveTab }}>
      {children}
    </TabsContext.Provider>
  )
}

export function TabList({ children }: { children: React.ReactNode }) {
  return <div className="tab-list">{children}</div>
}

export function Tab({ id, children }: { id: string, children: React.ReactNode }) {
  const context = useContext(TabsContext)
  if (!context) throw new Error('Tab must be used within Tabs')

  return (
    <button
      className={context.activeTab === id ? 'active' : ''}
      onClick={() => context.setActiveTab(id)}
    >
      {children}
    </button>
  )
}

// 使用
<Tabs defaultTab="overview">
  <TabList>
    <Tab id="overview">概览</Tab>
    <Tab id="details">详情</Tab>
  </TabList>
</Tabs>
```

### Render Props 模式

```typescript
interface DataLoaderProps<T> {
  url: string
  children: (data: T | null, loading: boolean, error: Error | null) => React.ReactNode
}

export function DataLoader<T>({ url, children }: DataLoaderProps<T>) {
  const [data, setData] = useState<T | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<Error | null>(null)

  useEffect(() => {
    fetch(url)
      .then(res => res.json())
      .then(setData)
      .catch(setError)
      .finally(() => setLoading(false))
  }, [url])

  return <>{children(data, loading, error)}</>
}

// 使用
<DataLoader<Market[]> url="/api/markets">
  {(markets, loading, error) => {
    if (loading) return <Spinner />
    if (error) return <Error error={error} />
    return <MarketList markets={markets!} />
  }}
</DataLoader>
```

## 自定义 Hook 模式

### 状态管理 Hook

```typescript
export function useToggle(initialValue = false): [boolean, () => void] {
  const [value, setValue] = useState(initialValue)

  const toggle = useCallback(() => {
    setValue(v => !v)
  }, [])

  return [value, toggle]
}

// 使用
const [isOpen, toggleOpen] = useToggle()
```

### 异步数据获取 Hook

```typescript
interface UseQueryOptions<T> {
  onSuccess?: (data: T) => void
  onError?: (error: Error) => void
  enabled?: boolean
}

export function useQuery<T>(
  key: string,
  fetcher: () => Promise<T>,
  options?: UseQueryOptions<T>
) {
  const [data, setData] = useState<T | null>(null)
  const [error, setError] = useState<Error | null>(null)
  const [loading, setLoading] = useState(false)

  const refetch = useCallback(async () => {
    setLoading(true)
    setError(null)

    try {
      const result = await fetcher()
      setData(result)
      options?.onSuccess?.(result)
    } catch (err) {
      const error = err as Error
      setError(error)
      options?.onError?.(error)
    } finally {
      setLoading(false)
    }
  }, [fetcher, options])

  useEffect(() => {
    if (options?.enabled !== false) {
      refetch()
    }
  }, [key, refetch, options?.enabled])

  return { data, error, loading, refetch }
}

// 使用
const { data: markets, loading, error, refetch } = useQuery(
  'markets',
  () => fetch('/api/markets').then(r => r.json()),
  {
    onSuccess: data => console.log('获取了', data.length, '个市场'),
    onError: err => console.error('失败:', err)
  }
)
```

### 防抖 Hook

```typescript
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
const [searchQuery, setSearchQuery] = useState('')
const debouncedQuery = useDebounce(searchQuery, 500)

useEffect(() => {
  if (debouncedQuery) {
    performSearch(debouncedQuery)
  }
}, [debouncedQuery])
```

## 状态管理模式

### Context + Reducer 模式

```typescript
interface State {
  markets: Market[]
  selectedMarket: Market | null
  loading: boolean
}

type Action =
  | { type: 'SET_MARKETS'; payload: Market[] }
  | { type: 'SELECT_MARKET'; payload: Market }
  | { type: 'SET_LOADING'; payload: boolean }

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case 'SET_MARKETS':
      return { ...state, markets: action.payload }
    case 'SELECT_MARKET':
      return { ...state, selectedMarket: action.payload }
    case 'SET_LOADING':
      return { ...state, loading: action.payload }
    default:
      return state
  }
}

const MarketContext = createContext<{
  state: State
  dispatch: Dispatch<Action>
} | undefined>(undefined)

export function MarketProvider({ children }: { children: React.ReactNode }) {
  const [state, dispatch] = useReducer(reducer, {
    markets: [],
    selectedMarket: null,
    loading: false
  })

  return (
    <MarketContext.Provider value={{ state, dispatch }}>
      {children}
    </MarketContext.Provider>
  )
}

export function useMarkets() {
  const context = useContext(MarketContext)
  if (!context) throw new Error('useMarkets must be used within MarketProvider')
  return context
}
```

## 性能优化

### 记忆化

```typescript
// 通过：expensive 计算使用 useMemo
const sortedMarkets = useMemo(() => {
  return markets.sort((a, b) => b.volume - a.volume)
}, [markets])

// 通过：传递给子组件的函数使用 useCallback
const handleSearch = useCallback((query: string) => {
  setSearchQuery(query)
}, [])

// 通过：纯组件使用 React.memo
export const MarketCard = React.memo<MarketCardProps>(({ market }) => {
  return (
    <div className="market-card">
      <h3>{market.name}</h3>
      <p>{market.description}</p>
    </div>
  )
})
```

### 代码分割和懒加载

```typescript
import { lazy, Suspense } from 'react'

// 通过：懒加载重型组件
const HeavyChart = lazy(() => import('./HeavyChart'))
const ThreeJsBackground = lazy(() => import('./ThreeJsBackground'))

export function Dashboard() {
  return (
    <div>
      <Suspense fallback={<ChartSkeleton />}>
        <HeavyChart data={data} />
      </Suspense>

      <Suspense fallback={null}>
        <ThreeJsBackground />
      </Suspense>
    </div>
  )
}
```

### 长列表虚拟化

```typescript
import { useVirtualizer } from '@tanstack/react-virtual'

export function VirtualMarketList({ markets }: { markets: Market[] }) {
  const parentRef = useRef<HTMLDivElement>(null)

  const virtualizer = useVirtualizer({
    count: markets.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 80,
    overscan: 5
  })

  return (
    <div ref={parentRef} style={{ height: '400px', overflow: 'auto' }}>
      <div style={{ height: `${virtualizer.getTotalSize()}px` }}>
        {virtualizer.getVirtualItems().map((virtualRow) => (
          <div
            key={virtualRow.key}
            style={{
              position: 'absolute',
              top: virtualRow.start,
              height: `${virtualRow.size}px`
            }}
          >
            <MarketRow market={markets[virtualRow.index]} />
          </div>
        ))}
      </div>
    </div>
  )
}
```

## 表单处理

### 受控输入

```typescript
export function LoginForm() {
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault()
    // 处理提交
  }

  return (
    <form onSubmit={handleSubmit}>
      <input
        type="email"
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        required
      />
      <input
        type="password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        required
      />
      <button type="submit">登录</button>
    </form>
  )
}
```

### Zod 验证

```typescript
import { z } from 'zod'

const loginSchema = z.object({
  email: z.string().email('无效的邮箱格式'),
  password: z.string().min(8, '密码至少 8 个字符')
})

type LoginForm = z.infer<typeof loginSchema>

export function LoginForm() {
  const [errors, setErrors] = useState<Record<string, string>>({})

  const handleSubmit = (data: unknown) => {
    const result = loginSchema.safeParse(data)
    if (!result.success) {
      const fieldErrors: Record<string, string> = {}
      result.error.errors.forEach((err) => {
        if (err.path[0]) {
          fieldErrors[err.path[0] as string] = err.message
        }
      })
      setErrors(fieldErrors)
      return
    }
    // 表单提交成功
  }

  return (
    <form onSubmit={handleSubmit}>
      <input name="email" type="email" />
      {errors.email && <span className="error">{errors.email}</span>}

      <input name="password" type="password" />
      {errors.password && <span className="error">{errors.password}</span>}

      <button type="submit">登录</button>
    </form>
  )
}
```

## 路由模式

### Next.js App Router

```typescript
// app/dashboard/page.tsx
export default function DashboardPage() {
  return (
    <div>
      <h1>仪表板</h1>
    </div>
  )
}

// app/dashboard/[marketId]/page.tsx
interface PageProps {
  params: { marketId: string }
}

export default function MarketDetailPage({ params }: PageProps) {
  const { marketId } = params
  return <div>市场 ID: {marketId}</div>
}
```

### React Router

```typescript
import { BrowserRouter, Routes, Route, Link, useParams } from 'react-router-dom'

export function App() {
  return (
    <BrowserRouter>
      <nav>
        <Link to="/">首页</Link>
        <Link to="/markets">市场</Link>
      </nav>

      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/markets" element={<MarketsPage />} />
        <Route path="/markets/:id" element={<MarketDetailPage />} />
      </Routes>
    </BrowserRouter>
  )
}

function MarketDetailPage() {
  const { id } = useParams()
  return <div>市场 {id}</div>
}
```

## 最佳实践

### 做

- 组件保持小而专注
- 状态尽可能本地化
- 昂贵的计算用 useMemo
- 回调函数用 useCallback
- 使用 TypeScript 避免类型错误

### 不要

- 不要创建巨大的单体组件
- 不要在不必要时使用 Context（性能开销）
- 不要在渲染中创建新函数或对象
- 不要忽略可访问性（a11y）
- 不要忘记响应式设计