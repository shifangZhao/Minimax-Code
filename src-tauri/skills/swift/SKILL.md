---
name: swiftui-patterns
description: SwiftUI 架构模式、@Observable 状态管理、视图组合、导航、性能优化和现代 iOS/macOS UI 最佳实践。
---

# SwiftUI 模式

用于在 Apple 平台上构建声明式、高性能用户界面的现代 SwiftUI 模式。涵盖 Observation 框架、视图组合、类型安全导航和性能优化。

## 激活时机

- 构建 SwiftUI 视图和管理状态（`@State`、`@Observable`、`@Binding`）
- 使用 `NavigationStack` 设计导航流程
- 构建视图模型和数据流
- 优化列表和复杂布局的渲染性能
- 在 SwiftUI 中使用环境值和依赖注入

## 状态管理

### 属性包装器选择

选择最简单的满足需求的包装器：

| 包装器 | 使用场景 |
|---------|----------|
| `@State` | 视图本地的值类型（开关、表单字段、sheet 呈现） |
| `@Binding` | 到父级 `@State` 的双向引用 |
| `@Observable` 类 + `@State` | 具有多个属性的自有模型 |
| `@Observable` 类（无包装器） | 从父级传递的只读引用 |
| `@Bindable` | 到 `@Observable` 属性的双向绑定 |
| `@Environment` | 通过 `.environment()` 注入的共享依赖 |

### @Observable ViewModel

使用 `@Observable`（不是 `ObservableObject`）— 它跟踪属性级变化，因此 SwiftUI 只重新渲染读取了已更改属性的视图：

```swift
@Observable
final class ItemListViewModel {
    private(set) var items: [Item] = []
    private(set) var isLoading = false
    var searchText = ""

    private let repository: any ItemRepository

    init(repository: any ItemRepository = DefaultItemRepository()) {
        self.repository = repository
    }

    func load() async {
        isLoading = true
        defer { isLoading = false }
        items = (try? await repository.fetchAll()) ?? []
    }
}
```

### 使用 ViewModel 的视图

```swift
struct ItemListView: View {
    @State private var viewModel: ItemListViewModel

    init(viewModel: ItemListViewModel = ItemListViewModel()) {
        _viewModel = State(initialValue: viewModel)
    }

    var body: some View {
        List(viewModel.items) { item in
            ItemRow(item: item)
        }
        .searchable(text: $viewModel.searchText)
        .overlay { if viewModel.isLoading { ProgressView() } }
        .task { await viewModel.load() }
    }
}
```

### 环境注入

用 `@Environment` 替换 `@EnvironmentObject`：

```swift
// 注入
ContentView()
    .environment(authManager)

// 消费
struct ProfileView: View {
    @Environment(AuthManager.self) private var auth

    var body: some View {
        Text(auth.currentUser?.name ?? "Guest")
    }
}
```

## 视图组合

### 提取子视图以限制失效

将视图拆分为小的、专注的结构。当状态改变时，只有读取该状态的子视图重新渲染：

```swift
struct OrderView: View {
    @State private var viewModel = OrderViewModel()

    var body: some View {
        VStack {
            OrderHeader(title: viewModel.title)
            OrderItemList(items: viewModel.items)
            OrderTotal(total: viewModel.total)
        }
    }
}
```

### ViewModifier 实现可重用样式

```swift
struct CardModifier: ViewModifier {
    func body(content: Content) -> some View {
        content
            .padding()
            .background(.regularMaterial)
            .clipShape(RoundedRectangle(cornerRadius: 12))
    }
}

extension View {
    func cardStyle() -> some View {
        modifier(CardModifier())
    }
}
```

## 导航

### 类型安全 NavigationStack

使用 `NavigationStack` 和 `NavigationPath` 实现程序化、类型安全的路由：

```swift
@Observable
final class Router {
    var path = NavigationPath()

    func navigate(to destination: Destination) {
        path.append(destination)
    }

    func popToRoot() {
        path = NavigationPath()
    }
}

enum Destination: Hashable {
    case detail(Item.ID)
    case settings
    case profile(User.ID)
}

struct RootView: View {
    @State private var router = Router()

    var body: some View {
        NavigationStack(path: $router.path) {
            HomeView()
                .navigationDestination(for: Destination.self) { dest in
                    switch dest {
                    case .detail(let id): ItemDetailView(itemID: id)
                    case .settings: SettingsView()
                    case .profile(let id): ProfileView(userID: id)
                    }
                }
        }
        .environment(router)
    }
}
```

## 性能

### 对大型集合使用懒容器

`LazyVStack` 和 `LazyHStack` 仅在可见时创建视图：

```swift
ScrollView {
    LazyVStack(spacing: 8) {
        ForEach(items) { item in
            ItemRow(item: item)
        }
    }
}
```

### 稳定标识符

在 `ForEach` 中始终使用稳定、唯一的 ID — 避免使用数组索引：

```swift
// 使用 Identifiable 一致或显式 id
ForEach(items, id: \.stableID) { item in
    ItemRow(item: item)
}
```

### 避免在 body 中执行昂贵工作

- 永远不要在 `body` 或 `init` 中直接执行 I/O、网络调用或重计算
- 使用 `.task {}` 执行异步工作 — 当视图消失时自动取消
- 在滚动视图中少用 `.sensoryFeedback()` 和 `.geometryGroup()`
- 在列表中最小化 `.shadow()`、`.blur()` 和 `.mask()` — 它们触发屏幕外渲染

### Equatable 一致性

对于有昂贵 body 的视图，实现 `Equatable` 以跳过不必要的重新渲染：

```swift
struct ExpensiveChartView: View, Equatable {
    let dataPoints: [DataPoint] // DataPoint 必须实现 Equatable

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.dataPoints == rhs.dataPoints
    }

    var body: some View {
        // 复杂的图表渲染
    }
}
```

## 预览

使用 `#Preview` 宏和内联模拟数据进行快速迭代：

```swift
#Preview("Empty state") {
    ItemListView(viewModel: ItemListViewModel(repository: EmptyMockRepository()))
}

#Preview("Loaded") {
    ItemListView(viewModel: ItemListViewModel(repository: PopulatedMockRepository()))
}
```

## 应避免的反模式

- 在新代码中使用 `ObservableObject` / `@Published` / `@StateObject` / `@EnvironmentObject` — 迁移到 `@Observable`
- 直接在 `body` 或 `init` 中放异步工作 — 使用 `.task {}` 或显式加载方法
- 在不拥有数据的子视图中将视图模型创建为 `@State` — 而是从父级传递
- 使用 `AnyView` 类型擦除 — 优先使用 `@ViewBuilder` 或 `Group` 实现条件视图
- 在向 actor 传递数据时忽略 `Sendable` 要求

## 参考

参见技能：`swift-actor-persistence` 用于基于 actor 的持久化模式。
参见技能：`swift-protocol-di-testing` 用于基于协议的 DI 和使用 Swift Testing 的测试。

---

---
name: swift-actor-persistence
description: 使用 actors 在 Swift 中实现线程安全的数据持久化 — 带文件备份存储的内存缓存，通过设计消除数据竞争。
origin: ECC
---

# Swift Actors 用于线程安全持久化

使用 Swift actors 构建线程安全数据持久化层的模式。结合内存缓存和文件备份存储，利用 actor 模型在编译时消除数据竞争。

## 激活时机

- 在 Swift 5.5+ 中构建数据持久化层
- 需要线程安全访问共享可变状态
- 想要消除手动同步（锁、DispatchQueues）
- 构建带本地存储的离线优先应用

## 核心模式

### 基于 Actor 的 Repository

Actor 模型保证序列化访问 — 无数据竞争，编译器强制执行。

```swift
public actor LocalRepository<T: Codable & Identifiable> where T.ID == String {
    private var cache: [String: T] = [:]
    private let fileURL: URL

    public init(directory: URL = .documentsDirectory, filename: String = "data.json") {
        self.fileURL = directory.appendingPathComponent(filename)
        // 在 init 期间同步加载（actor 隔离尚未激活）
        self.cache = Self.loadSynchronously(from: fileURL)
    }

    // MARK: - Public API

    public func save(_ item: T) throws {
        cache[item.id] = item
        try persistToFile()
    }

    public func delete(_ id: String) throws {
        cache[id] = nil
        try persistToFile()
    }

    public func find(by id: String) -> T? {
        cache[id]
    }

    public func loadAll() -> [T] {
        Array(cache.values)
    }

    // MARK: - Private

    private func persistToFile() throws {
        let data = try JSONEncoder().encode(Array(cache.values))
        try data.write(to: fileURL, options: .atomic)
    }

    private static func loadSynchronously(from url: URL) -> [String: T] {
        guard let data = try? Data(contentsOf: url),
              let items = try? JSONDecoder().decode([T].self, from: data) else {
            return [:]
        }
        return Dictionary(uniqueKeysWithValues: items.map { ($0.id, $0) })
    }
}
```

### 使用

所有调用由于 actor 隔离自动成为 async：

```swift
let repository = LocalRepository<Question>()

// 读取 — 从内存缓存快速 O(1) 查找
let question = await repository.find(by: "q-001")
let allQuestions = await repository.loadAll()

// 写入 — 更新缓存并原子持久化到文件
try await repository.save(newQuestion)
try await repository.delete("q-001")
```

### 与 @Observable ViewModel 结合

```swift
@Observable
final class QuestionListViewModel {
    private(set) var questions: [Question] = []
    private let repository: LocalRepository<Question>

    init(repository: LocalRepository<Question> = LocalRepository()) {
        self.repository = repository
    }

    func load() async {
        questions = await repository.loadAll()
    }

    func add(_ question: Question) async throws {
        try await repository.save(question)
        questions = await repository.loadAll()
    }
}
```

## 关键设计决策

| 决策 | 理由 |
|----------|-----------|
| Actor（不是类 + 锁） | 编译器强制线程安全，无手动同步 |
| 内存缓存 + 文件持久化 | 从缓存快速读取，向磁盘持久化写入 |
| 在 `init` 中同步加载 | 避免异步初始化复杂性 |
| 按 ID 键控的 Dictionary | 按标识符 O(1) 查找 |
| 泛型 `Codable & Identifiable` | 跨任何模型类型可重用 |
| 原子文件写入（`.atomic`） | 防止崩溃时部分写入 |

## 最佳实践

- **对所有跨 actor 边界的数据使用 `Sendable` 类型**
- **保持 actor 的公共 API 最小化** — 只暴露领域操作，不暴露持久化细节
- **使用 `.atomic` 写入** 防止应用在写入中间崩溃时数据损坏
- **在 `init` 中同步加载** — 异步初始化程序为本地文件增加最小利益的复杂性
- **与 `@Observable` ViewModel 结合** 实现响应式 UI 更新

## 应避免的反模式

- 在新 Swift 并发代码中使用 `DispatchQueue` 或 `NSLock` 而非 actors
- 向外部调用者暴露内部缓存 dictionary
- 无验证地使文件 URL 可配置
- 忘记所有 actor 方法调用都是 `await` — 调用者必须处理异步上下文
- 使用 `nonisolated` 绕过 actor 隔离（违背目的）

## 使用场景

- iOS/macOS 应用中的本地数据存储（用户数据、设置、缓存内容）
- 稍后同步到服务器的离线优先架构
- 应用多个部分并发访问的任何共享可变状态
- 用现代 Swift 并发替换遗留的基于 `DispatchQueue` 的线程安全

---

---
name: swift-concurrency-6-2
description: Swift 6.2 亲和性并发 — 默认单线程，@concurrent 用于显式后台卸载，MainActor 类型的隔离一致。
---

# Swift 6.2 亲和性并发

用于采用 Swift 6.2 并发模型的模式，其中代码默认单线程运行，并发是显式引入的。消除常见数据竞争错误而不牺牲性能。

## 激活时机

- 将 Swift 5.x 或 6.0/6.1 项目迁移到 Swift 6.2
- 解决数据竞争安全编译器错误
- 设计 MainActor 基于的应用架构
- 将 CPU 密集型工作卸载到后台线程
- 在 MainActor 隔离的类型上实现协议一致性
- 在 Xcode 26 中启用亲和性并发构建设置

## 核心问题：隐式后台卸载

在 Swift 6.1 及更早版本中，async 函数可能隐式卸载到后台线程，导致看似安全的代码中出现数据竞争错误：

```swift
// Swift 6.1: ERROR
@MainActor
final class StickerModel {
    let photoProcessor = PhotoProcessor()

    func extractSticker(_ item: PhotosPickerItem) async throws -> Sticker? {
        guard let data = try await item.loadTransferable(type: Data.self) else { return nil }

        // Error: Sending 'self.photoProcessor' risks causing data races
        return await photoProcessor.extractSticker(data: data, with: item.itemIdentifier)
    }
}
```

Swift 6.2 修复了这个问题：async 函数默认保持在调用 actor 上。

```swift
// Swift 6.2: OK — async 保持在 MainActor 上，无数据竞争
@MainActor
final class StickerModel {
    let photoProcessor = PhotoProcessor()

    func extractSticker(_ item: PhotosPickerItem) async throws -> Sticker? {
        guard let data = try await item.loadTransferable(type: Data.self) else { return nil }
        return await photoProcessor.extractSticker(data: data, with: item.itemIdentifier)
    }
}
```

## 核心模式 — 隔离一致

MainActor 类型现在可以安全地实现非隔离协议：

```swift
protocol Exportable {
    func export()
}

// Swift 6.1: ERROR — 进入 main actor 隔离代码
// Swift 6.2: 带隔离一致的 OK
extension StickerModel: @MainActor Exportable {
    func export() {
        photoProcessor.exportAsPNG()
    }
}
```

编译器确保该一致性仅在 main actor 上使用：

```swift
// OK — ImageExporter 也是 @MainActor
@MainActor
struct ImageExporter {
    var items: [any Exportable]

    mutating func add(_ item: StickerModel) {
        items.append(item)  // 安全：相同的 actor 隔离
    }
}

// ERROR — nonisolated 上下文不能使用 MainActor 一致性
nonisolated struct ImageExporter {
    var items: [any Exportable]

    mutating func add(_ item: StickerModel) {
        items.append(item)  // Error: Main actor-isolated consistency cannot be used here
    }
}
```

## 核心模式 — 全局和静态变量

用 MainActor 保护全局/静态状态：

```swift
// Swift 6.1: ERROR — 非 Sendable 类型可能有共享可变状态
final class StickerLibrary {
    static let shared: StickerLibrary = .init()  // Error
}

// 修复：用 @MainActor 标注
@MainActor
final class StickerLibrary {
    static let shared: StickerLibrary = .init()  // OK
}
```

### MainActor 默认推理模式

Swift 6.2 引入了一种默认推理 MainActor 的模式 — 无需手动标注：

```swift
// 启用 MainActor 默认推理时：
final class StickerLibrary {
    static let shared: StickerLibrary = .init()  // 隐式 @MainActor
}

final class StickerModel {
    let photoProcessor: PhotoProcessor
    var selection: [PhotosPickerItem]  // 隐式 @MainActor
}

extension StickerModel: Exportable {  // 隐式 @MainActor 一致性
    func export() {
        photoProcessor.exportAsPNG()
    }
}
```

此模式是可选的，推荐用于应用、脚本和其他可执行目标。

## 核心模式 — @concurrent 用于后台工作

当你需要实际并行时，用 `@concurrent` 显式卸载：

> **重要**：此示例需要亲和性并发构建设置 — SE-0466（MainActor 默认隔离）和 SE-0461（NonisolatedNonsendingByDefault）。启用这些后，`extractSticker` 保持在调用者的 actor 上，使可变状态访问安全。**没有这些设置，此代码有数据竞争** — 编译器会标记它。

```swift
nonisolated final class PhotoProcessor {
    private var cachedStickers: [String: Sticker] = [:]

    func extractSticker(data: Data, with id: String) async -> Sticker {
        if let sticker = cachedStickers[id] {
            return sticker
        }

        let sticker = await Self.extractSubject(from: data)
        cachedStickers[id] = sticker
        return sticker
    }

    // 将昂贵工作卸载到并发线程池
    @concurrent
    static func extractSubject(from data: Data) async -> Sticker { /* ... */ }
}

// 调用者必须 await
let processor = PhotoProcessor()
processedPhotos[item.id] = await processor.extractSticker(data: data, with: item.id)
```

使用 `@concurrent`：
1. 将包含类型标注为 `nonisolated`
2. 给函数添加 `@concurrent`
3. 如果尚未异步则添加 `async`
4. 在调用点添加 `await`

## 关键设计决策

| 决策 | 理由 |
|----------|-----------|
| 默认单线程 | 最自然的代码无数据竞争；并发是选用的 |
| Async 保持在调用 actor | 消除导致数据竞争错误的隐式卸载 |
| 隔离一致 | MainActor 类型可以安全实现协议，无不安全变通方案 |
| `@concurrent` 显式选用 | 后台执行是深思熟虑的性能选择，非偶然 |
| MainActor 默认推理 | 减少应用目标的 `@MainActor` 标注样板 |
| 选用采用 | 非破坏性迁移路径 — 增量启用功能 |

## 迁移步骤

1. **在 Xcode 中启用**：Swift Compiler > Concurrency 部分在构建设置中
2. **在 SPM 中启用**：在包清单中使用 `SwiftSettings` API
3. **使用迁移工具**：通过 swift.org/migration 自动代码更改
4. **从 MainActor 默认开始**：为应用目标启用推理模式
5. **在需要时添加 `@concurrent`**：先分析性能，然后卸载热路径
6. **彻底测试**：数据竞争问题变为编译时错误

## 最佳实践

- **从 MainActor 开始** — 先写单线程代码，之后优化
- **仅对 CPU 密集型工作使用 `@concurrent`** — 图像处理、压缩、复杂计算
- **为应用目标启用 MainActor 推理模式**，这些目标大多为单线程
- **先分析再卸载** — 使用 Instruments 找到实际瓶颈
- **用 MainActor 保护全局** — 全局/静态可变状态需要 actor 隔离
- **使用隔离一致** 而非 `nonisolated` 变通方案或 `@Sendable` 包装器
- **增量迁移** — 在构建设置中一次启用一个功能

## 应避免的反模式

- 对每个 async 函数应用 `@concurrent`（大多数不需要后台执行）
- 在不理解隔离的情况下使用 `nonisolated` 抑制编译器错误
- 当 actors 提供相同安全性时保持遗留 `DispatchQueue` 模式
- 在并发相关 Foundation Models 代码中跳过 `model.availability` 检查
- 与编译器对抗 — 如果它报告数据竞争，代码有真正的并发问题
- 假设所有 async 代码都在后台运行（Swift 6.2 默认：保持在调用 actor）

## 使用场景

- 所有新的 Swift 6.2+ 项目（亲和性并发是推荐默认）
- 从 Swift 5.x 或 6.0/6.1 并发迁移现有应用
- 在 Xcode 26 采用期间解决数据竞争安全编译器错误
- 构建 MainActor 中心的应用架构（大多数 UI 应用）
- 性能优化 — 将特定重计算卸载到后台

---

---
name: swift-protocol-di-testing
description: 用于可测试 Swift 代码的基于协议的依赖注入 — 使用专注的协议和 Swift Testing mock 文件系统、网络和外部 API。
origin: ECC
---

# Swift 基于协议的依赖注入测试

通过将外部依赖（文件系统、网络、iCloud）抽象为小的、专注的协议来使 Swift 代码可测试的模式。无需 I/O 即可实现确定性测试。

## 激活时机

- 编写访问文件系统、网络或外部 API 的 Swift 代码
- 需要测试错误处理路径而不触发真实失败
- 构建跨环境（应用、测试、SwiftUI 预览）工作的模块
- 使用 Swift 并发（actors、Sendable）设计可测试架构

## 核心模式

### 1. 定义小而专注的协议

每个协议处理一个外部关注点。

```swift
// 文件系统访问
public protocol FileSystemProviding: Sendable {
    func containerURL(for purpose: Purpose) -> URL?
}

// 文件读/写操作
public protocol FileAccessorProviding: Sendable {
    func read(from url: URL) throws -> Data
    func write(_ data: Data, to url: URL) throws
    func fileExists(at url: URL) -> Bool
}

// 书签存储（例如，用于沙盒应用）
public protocol BookmarkStorageProviding: Sendable {
    func saveBookmark(_ data: Data, for key: String) throws
    func loadBookmark(for key: String) throws -> Data?
}
```

### 2. 创建默认（生产）实现

```swift
public struct DefaultFileSystemProvider: FileSystemProviding {
    public init() {}

    public func containerURL(for purpose: Purpose) -> URL? {
        FileManager.default.url(forUbiquityContainerIdentifier: nil)
    }
}

public struct DefaultFileAccessor: FileAccessorProviding {
    public init() {}

    public func read(from url: URL) throws -> Data {
        try Data(contentsOf: url)
    }

    public func write(_ data: Data, to url: URL) throws {
        try data.write(to: url, options: .atomic)
    }

    public func fileExists(at url: URL) -> Bool {
        FileManager.default.fileExists(atPath: url.path)
    }
}
```

### 3. 为测试创建 Mock 实现

```swift
public final class MockFileAccessor: FileAccessorProviding, @unchecked Sendable {
    public var files: [URL: Data] = [:]
    public var readError: Error?
    public var writeError: Error?

    public init() {}

    public func read(from url: URL) throws -> Data {
        if let error = readError { throw error }
        guard let data = files[url] else {
            throw CocoaError(.fileReadNoSuchFile)
        }
        return data
    }

    public func write(_ data: Data, to url: URL) throws {
        if let error = writeError { throw error }
        files[url] = data
    }

    public func fileExists(at url: URL) -> Bool {
        files[url] != nil
    }
}
```

### 4. 用默认参数注入依赖

生产代码使用默认值；测试注入 mocks。

```swift
public actor SyncManager {
    private let fileSystem: FileSystemProviding
    private let fileAccessor: FileAccessorProviding

    public init(
        fileSystem: FileSystemProviding = DefaultFileSystemProvider(),
        fileAccessor: FileAccessorProviding = DefaultFileAccessor()
    ) {
        self.fileSystem = fileSystem
        self.fileAccessor = fileAccessor
    }

    public func sync() async throws {
        guard let containerURL = fileSystem.containerURL(for: .sync) else {
            throw SyncError.containerNotAvailable
        }
        let data = try fileAccessor.read(
            from: containerURL.appendingPathComponent("data.json")
        )
        // 处理数据...
    }
}
```

### 5. 用 Swift Testing 编写测试

```swift
import Testing

@Test("Sync manager handles missing container")
func testMissingContainer() async {
    let mockFileSystem = MockFileSystemProvider(containerURL: nil)
    let manager = SyncManager(fileSystem: mockFileSystem)

    await #expect(throws: SyncError.containerNotAvailable) {
        try await manager.sync()
    }
}

@Test("Sync manager reads data correctly")
func testReadData() async throws {
    let mockFileAccessor = MockFileAccessor()
    mockFileAccessor.files[testURL] = testData

    let manager = SyncManager(fileAccessor: mockFileAccessor)
    let result = try await manager.loadData()

    #expect(result == expectedData)
}

@Test("Sync manager handles read errors gracefully")
func testReadError() async {
    let mockFileAccessor = MockFileAccessor()
    mockFileAccessor.readError = CocoaError(.fileReadCorruptFile)

    let manager = SyncManager(fileAccessor: mockFileAccessor)

    await #expect(throws: SyncError.self) {
        try await manager.sync()
    }
}
```

## 最佳实践

- **单一职责**：每个协议应处理一个关注点 — 不要创建具有许多方法的"上帝协议"
- **Sendable 一致性**：在 actor 边界使用协议时需要
- **默认参数**：让生产代码默认使用真实实现；只有测试需要指定 mocks
- **错误模拟**：设计具有可配置错误属性的 mocks 以测试失败路径
- **只 mock 边界**：Mock 外部依赖（文件系统、网络、API），而非内部类型

## 应避免的反模式

- 创建覆盖所有外部访问的单一大型协议
- Mock 没有外部依赖的内部类型
- 使用 `#if DEBUG` 条件而非适当的依赖注入
- 与 actors 一起使用忘记 `Sendable` 一致性
- 过度工程：如果类型没有外部依赖，它不需要协议

## 使用场景

- 任何接触文件系统、网络或外部 API 的 Swift 代码
- 难以在真实环境中触发的错误处理路径测试
- 需要在应用、测试和 SwiftUI 预览上下文中工作的模块