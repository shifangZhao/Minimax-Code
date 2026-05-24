---
name: nuxt4-patterns
description: Nuxt 4 应用模式，用于水合安全、性能、路由规则、惰性加载和使用 useFetch 和 useAsyncData 的 SSR 安全数据获取。
origin: ECC
---

# Nuxt 4 模式

在构建或调试具有 SSR、混合渲染、路由规则或页面级数据获取的 Nuxt 4 应用时使用。

## 激活时机

- 服务端渲染 HTML 和客户端状态之间的水合不匹配
- 路由级渲染决策如预渲染、SWR、ISR 或仅客户端部分
- 围绕惰性加载、惰性水合或有效载荷大小的性能工作
- 使用 `useFetch`、`useAsyncData` 或 `$fetch` 的页面或组件数据获取
- 与路由参数、中间件或 SSR/客户端差异相关的 Nuxt 路由问题

## 水合安全

- 保持第一次渲染确定性。不要将 `Date.now()`、`Math.random()`、仅浏览器 API 或存储读取直接放入 SSR 渲染的模板状态。
- 当服务器无法产生相同标记时，将仅浏览器逻辑移至 `onMounted()`、`import.meta.client`、`ClientOnly` 或 `.client.vue` 组件。
- 使用 Nuxt 的 `useRoute()` composable，而非来自 `vue-router` 的。
- 不要使用 `route.fullPath` 驱动 SSR 渲染的标记。URL 片段是仅客户端的，可能创建水合不匹配。
- 将 `ssr: false` 视为真正仅浏览器区域的逃生舱，而非修复不匹配的默认方法。

## 数据获取

- 优先 `await useFetch()` 用于页面和组件中 SSR 安全的 API 读取。它将服务端获取的数据转发到 Nuxt 有效载荷，避免在水合时第二次获取。
- 当 fetcher 不是简单的 `$fetch()` 调用、需要自定义 key 或组合多个异步源时使用 `useAsyncData()`。
- 为 `useAsyncData()` 提供稳定 key 以便缓存重用和可预测刷新行为。
- 保持 `useAsyncData()` 处理程序无副作用。它们可以在 SSR 和水合期间运行。
- 对用户触发的写入或仅客户端操作使用 `$fetch()`，而非应该从 SSR 水合的顶级页面数据。
- 对非关键数据使用 `lazy: true`、`useLazyFetch()` 或 `useLazyAsyncData()`，不应阻止导航。在 UI 中处理 `status === 'pending'`。
- 仅对不需要 SEO 或首次绘制的数据使用 `server: false`。
- 使用 `pick` 修剪有效载荷大小，优先较浅的有效载荷而非深度响应式不必要时。

```ts
const route = useRoute()

const { data: article, status, error, refresh } = await useAsyncData(
  () => `article:${route.params.slug}`,
  () => $fetch(`/api/articles/${route.params.slug}`),
)

const { data: comments } = await useFetch(`/api/articles/${route.params.slug}/comments`, {
  lazy: true,
  server: false,
})
```

## 路由规则

在 `nuxt.config.ts` 中优先使用 `routeRules` 用于渲染和缓存策略：

```ts
export default defineNuxtConfig({
  routeRules: {
    '/': { prerender: true },
    '/products/**': { swr: 3600 },
    '/blog/**': { isr: true },
    '/admin/**': { ssr: false },
    '/api/**': { cache: { maxAge: 60 * 60 } },
  },
})
```

- `prerender`：构建时静态 HTML
- `swr`：服务缓存内容并在后台重新验证
- `isr`：在支持平台上增量静态再生成
- `ssr: false`：客户端渲染路由
- `cache` 或 `redirect`：Nitro 级响应行为

按路由组选择路由规则，而非全局。营销页面、目录、仪表板和 API 通常需要不同策略。

## 惰性加载和性能

- Nuxt 已按路由代码分割页面。在 micro-optimizing 组件分割之前保持路由边界有意义。
- 使用 `Lazy` 前缀动态导入非关键组件。
- 使用 `v-if` 条件渲染惰性组件，以便在 UI 真正需要之前不加载 chunk。
- 对折叠下方或非关键交互式 UI 使用惰性水合。

```vue
<template>
  <LazyRecommendations v-if="showRecommendations" />
  <LazyProductGallery hydrate-on-visible />
</template>
```

- 对自定义策略，使用带有可见性或空闲策略的 `defineLazyHydrationComponent()`。
- Nuxt 惰性水合在单文件组件上工作。传递新 props 给惰性水合组件会立即触发水合。
- 使用 `NuxtLink` 进行内部导航，以便 Nuxt 可以预取路由组件和生成的有效载荷。

## 审查检查清单

- 第一次 SSR 渲染和水合客户端渲染产生相同标记
- 页面数据使用 `useFetch` 或 `useAsyncData`，而非顶级 `$fetch`
- 非关键数据是惰性的并有显式加载 UI
- 路由规则匹配页面的 SEO 和新鲜度要求
- 重交互岛屿是惰性加载或惰性水合的