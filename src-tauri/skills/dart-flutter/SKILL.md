---
name: dart-flutter-patterns
description: 生产级 Dart 和 Flutter 模式，涵盖空安全、不可变状态、异步组合、widget 架构、流行状态管理框架（BLoC、Riverpod、Provider）、GoRouter 导航、Dio 网络、Freezed 代码生成和清洁架构。
origin: ECC
---

# Dart/Flutter 模式

## 使用场景

在以下情况使用此技能：
- 启动新的 Flutter 功能，需要状态管理、导航或数据访问的惯用模式
- 审查或编写 Dart 代码，需要空安全、密封类型或异步组合的指导
- 设置新的 Flutter 项目，在 BLoC、Riverpod 或 Provider 之间选择
- 实现安全 HTTP 客户端、WebView 集成或本地存储
- 为 Flutter widget、Cubit 或 Riverpod 提供者编写测试
- 使用身份验证守卫连接 GoRouter

## 工作原理

此技能提供按关注点组织的可复制粘贴的 Dart/Flutter 代码模式：
1. **空安全** — 避免 `!`，优先 `?.`/`??`/模式匹配
2. **不可变状态** — 密封类、`freezed`、`copyWith`
3. **异步组合** — 并发 `Future.wait`、await 后安全使用 `BuildContext`
4. **Widget 架构** — 提取为类（而非方法）、`const` 传播、作用域重建
5. **状态管理** — BLoC/Cubit 事件、Riverpod notifiers 和派生提供者
6. **导航** — 带响应式身份验证守卫的 GoRouter（通过 `refreshListenable`）
7. **网络** — 带拦截器、Dio，一次性重试保护的 token 刷新
8. **错误处理** — 全局捕获、`ErrorWidget.builder`、crashlytics 连接
9. **测试** — 单元（BLoC 测试）、widget（ProviderScope 覆盖）、fake 优于 mock

## 示例

```dart
// 密封状态 — 防止不可能状态
sealed class AsyncState<T> {}
final class Loading<T> extends AsyncState<T> {}
final class Success<T> extends AsyncState<T> { final T data; const Success(this.data); }
final class Failure<T> extends AsyncState<T> { final Object error; const Failure(this.error); }

// 带响应式身份验证重定向的 GoRouter
final router = GoRouter(
  refreshListenable: GoRouterRefreshStream(authCubit.stream),
  redirect: (context, state) {
    final authed = context.read<AuthCubit>().state is AuthAuthenticated;
    if (!authed && !state.matchedLocation.startsWith('/login')) return '/login';
    return null;
  },
  routes: [...],
);

// Riverpod 派生提供者，带安全的 firstWhereOrNull
@riverpod
double cartTotal(Ref ref) {
  final cart = ref.watch(cartNotifierProvider);
  final products = ref.watch(productsProvider).valueOrNull ?? [];
  return cart.fold(0.0, (total, item) {
    final product = products.firstWhereOrNull((p) => p.id == item.productId);
    return total + (product?.price ?? 0) * item.quantity;
  });
}
```

---

Dart 和 Flutter 应用的生产级实用模式。在可能的情况下与库无关，并明确覆盖最常见的生态系统包。

---

## 1. 空安全基础

### 优先使用模式而非 Bang 操作符

```dart
// 坏 — 如果为 null 则在运行时崩溃
final name = user!.name;

// 好 — 提供后备
final name = user?.name ?? 'Unknown';

// 好 — Dart 3 模式匹配（复杂情况首选）
final display = switch (user) {
  User(:final name, :final email) => '$name <$email>',
  null => 'Guest',
};

// 好 — 提前 guard 返回
String getUserName(User? user) {
  if (user == null) return 'Unknown';
  return user.name; // 检查后升级为非 null
}
```

### 避免过度使用 `late`

```dart
// 坏 — 将 null 错误推迟到运行时
late String userId;

// 好 — 可空类型，显式初始化
String? userId;

// 可以 — 仅当初始化保证在首次访问之前时使用 late
// （例如在 initState() 中在任何 widget 交互之前）
late final AnimationController _controller;

@override
void initState() {
  super.initState();
  _controller = AnimationController(vsync: this, duration: const Duration(milliseconds: 300));
}
```

---

## 2. 不可变状态

### 用于状态层次结构的密封类

```dart
sealed class UserState {}

final class UserInitial extends UserState {}

final class UserLoading extends UserState {}

final class UserLoaded extends UserState {
  const UserLoaded(this.user);
  final User user;
}

final class UserError extends UserState {
  const UserError(this.message);
  final String message;
}

// 穷举 switch — 编译器强制所有分支
Widget buildFrom(UserState state) => switch (state) {
  UserInitial() => const SizedBox.shrink(),
  UserLoading() => const CircularProgressIndicator(),
  UserLoaded(:final user) => UserCard(user: user),
  UserError(:final message) => ErrorText(message),
};
```

### Freezed 实现无样板不可变性

```dart
import 'package:freezed_annotation/freezed_annotation.dart';

part 'user.freezed.dart';
part 'user.g.dart';

@freezed
class User with _$User {
  const factory User({
    required String id,
    required String name,
    required String email,
    @Default(false) bool isAdmin,
  }) = _User;

  factory User.fromJson(Map<String, dynamic> json) => _$UserFromJson(json);
}

// 使用
final user = User(id: '1', name: 'Alice', email: 'alice@example.com');
final updated = user.copyWith(name: 'Alice Smith'); // 不可变更新
final json = user.toJson();
final fromJson = User.fromJson(json);
```

---

## 3. 异步组合

### 使用 Future.wait 的结构化并发

```dart
Future<DashboardData> loadDashboard(UserRepository users, OrderRepository orders) async {
  // 并发运行 — 不要顺序 await
  final (userList, orderList) = await (
    users.getAll(),
    orders.getRecent(),
  ).wait; // Dart 3 记录解构 + Future.wait 扩展

  return DashboardData(users: userList, orders: orderList);
}
```

### Stream 模式

```dart
// 仓库暴露响应式流以获取实时数据
Stream<List<Item>> watchCartItems() => _db
    .watchTable('cart_items')
    .map((rows) => rows.map(Item.fromRow).toList());

// 在 widget 层 — 声明式，无手动订阅
StreamBuilder<List<Item>>(
  stream: cartRepository.watchCartItems(),
  builder: (context, snapshot) => switch (snapshot) {
    AsyncSnapshot(connectionState: ConnectionState.waiting) =>
        const CircularProgressIndicator(),
    AsyncSnapshot(:final error?) => ErrorWidget(error.toString()),
    AsyncSnapshot(:final data?) => CartList(items: data),
    _ => const SizedBox.shrink(),
  },
)
```

### Await 后使用 BuildContext

```dart
// 关键 — 在 StatefulWidget 中任何 await 后始终检查 mounted
Future<void> _handleSubmit() async {
  setState(() => _isLoading = true);
  try {
    await authService.login(_email, _password);
    if (!mounted) return; // ← 在使用 context 前 guard
    context.go('/home');
  } on AuthException catch (e) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(e.message)));
  } finally {
    if (mounted) setState(() => _isLoading = false);
  }
}
```

---

## 4. Widget 架构

### 提取为类，而非方法

```dart
// 坏 — 返回 widget 的私有方法，阻止优化
Widget _buildHeader() {
  return Container(
    padding: const EdgeInsets.all(16),
    child: Text(title, style: Theme.of(context).textTheme.headlineMedium),
  );
}

// 好 — 单独的 widget 类，支持 const、元素复用
class _PageHeader extends StatelessWidget {
  const _PageHeader(this.title);
  final String title;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(16),
      child: Text(title, style: Theme.of(context).textTheme.headlineMedium),
    );
  }
}
```

### const 传播

```dart
// 坏 — 每次重建创建新实例
child: Padding(
  padding: EdgeInsets.all(16.0),       // not const
  child: Icon(Icons.home, size: 24.0), // not const
)

// 好 — const 阻止重建传播
child: const Padding(
  padding: EdgeInsets.all(16.0),
  child: Icon(Icons.home, size: 24.0),
)
```

### 作用域重建

```dart
// 坏 — 每次计数器更改都重建整个页面
class CounterPage extends ConsumerWidget {
  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final count = ref.watch(counterProvider); // 重建所有内容
    return Scaffold(
      body: Column(children: [
        const ExpensiveHeader(), // 不必要地重建
        Text('$count'),
        const ExpensiveFooter(), // 不必要地重建
      ]),
    );
  }
}

// 好 — 隔离重建部分
class CounterPage extends StatelessWidget {
  const CounterPage({super.key});

  @override
  Widget build(BuildContext context) {
    return const Scaffold(
      body: Column(children: [
        ExpensiveHeader(),        // 从不重建（const）
        _CounterDisplay(),         // 只有这个重建
        ExpensiveFooter(),        // 从不重建（const）
      ]),
    );
  }
}

class _CounterDisplay extends ConsumerWidget {
  const _CounterDisplay();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final count = ref.watch(counterProvider);
    return Text('$count');
  }
}
```

---

## 5. 状态管理：BLoC/Cubit

```dart
// Cubit — 同步或简单异步状态
class AuthCubit extends Cubit<AuthState> {
  AuthCubit(this._authService) : super(const AuthState.initial());
  final AuthService _authService;

  Future<void> login(String email, String password) async {
    emit(const AuthState.loading());
    try {
      final user = await _authService.login(email, password);
      emit(AuthState.authenticated(user));
    } on AuthException catch (e) {
      emit(AuthState.error(e.message));
    }
  }

  void logout() {
    _authService.logout();
    emit(const AuthState.initial());
  }
}

// 在 widget 中
BlocBuilder<AuthCubit, AuthState>(
  builder: (context, state) => switch (state) {
    AuthInitial() => const LoginForm(),
    AuthLoading() => const CircularProgressIndicator(),
    AuthAuthenticated(:final user) => HomePage(user: user),
    AuthError(:final message) => ErrorView(message: message),
  },
)
```

---

## 6. 状态管理：Riverpod

```dart
// 自动释放异步提供者
@riverpod
Future<List<Product>> products(Ref ref) async {
  final repo = ref.watch(productRepositoryProvider);
  return repo.getAll();
}

// 带复杂变更的 Notifier
@riverpod
class CartNotifier extends _$CartNotifier {
  @override
  List<CartItem> build() => [];

  void add(Product product) {
    final existing = state.where((i) => i.productId == product.id).firstOrNull;
    if (existing != null) {
      state = [
        for (final item in state)
          if (item.productId == product.id) item.copyWith(quantity: item.quantity + 1)
          else item,
      ];
    } else {
      state = [...state, CartItem(productId: product.id, quantity: 1)];
    }
  }

  void remove(String productId) =>
      state = state.where((i) => i.productId != productId).toList();

  void clear() => state = [];
}

// 派生提供者（选择器模式）
@riverpod
int cartCount(Ref ref) => ref.watch(cartNotifierProvider).length;

@riverpod
double cartTotal(Ref ref) {
  final cart = ref.watch(cartNotifierProvider);
  final products = ref.watch(productsProvider).valueOrNull ?? [];
  return cart.fold(0.0, (total, item) {
    // firstWhereOrNull（来自 collection 包）在产品缺失时避免 StateError
    final product = products.firstWhereOrNull((p) => p.id == item.productId);
    return total + (product?.price ?? 0) * item.quantity;
  });
}
```

---

## 7. GoRouter 导航

```dart
final router = GoRouter(
  initialLocation: '/',
  // refreshListenable 在 auth 状态更改时重新评估 redirect
  refreshListenable: GoRouterRefreshStream(authCubit.stream),
  redirect: (context, state) {
    final isLoggedIn = context.read<AuthCubit>().state is AuthAuthenticated;
    final isGoingToLogin = state.matchedLocation == '/login';
    if (!isLoggedIn && !isGoingToLogin) return '/login';
    if (isLoggedIn && isGoingToLogin) return '/';
    return null;
  },
  routes: [
    GoRoute(path: '/login', builder: (_, __) => const LoginPage()),
    ShellRoute(
      builder: (context, state, child) => AppShell(child: child),
      routes: [
        GoRoute(path: '/', builder: (_, __) => const HomePage()),
        GoRoute(
          path: '/products/:id',
          builder: (context, state) =>
              ProductDetailPage(id: state.pathParameters['id']!),
        ),
      ],
    ),
  ],
);
```

---

## 8. Dio HTTP

```dart
final dio = Dio(BaseOptions(
  baseUrl: const String.fromEnvironment('API_URL'),
  connectTimeout: const Duration(seconds: 10),
  receiveTimeout: const Duration(seconds: 30),
  headers: {'Content-Type': 'application/json'},
));

// 添加 auth 拦截器
dio.interceptors.add(InterceptorsWrapper(
  onRequest: (options, handler) async {
    final token = await secureStorage.read(key: 'auth_token');
    if (token != null) options.headers['Authorization'] = 'Bearer $token';
    handler.next(options);
  },
  onError: (error, handler) async {
    // Guard against infinite retry loops: only attempt refresh once per request
    final isRetry = error.requestOptions.extra['_isRetry'] == true;
    if (!isRetry && error.response?.statusCode == 401) {
      final refreshed = await attemptTokenRefresh();
      if (refreshed) {
        error.requestOptions.extra['_isRetry'] = true;
        return handler.resolve(await dio.fetch(error.requestOptions));
      }
    }
    handler.next(error);
  },
));

// 使用 Dio 的仓库
class UserApiDataSource {
  const UserApiDataSource(this._dio);
  final Dio _dio;

  Future<User> getById(String id) async {
    final response = await _dio.get<Map<String, dynamic>>('/users/$id');
    return User.fromJson(response.data!);
  }
}
```

---

## 9. 错误处理架构

```dart
// 全局错误捕获 — 在 main() 中设置
void main() {
  FlutterError.onError = (details) {
    FlutterError.presentError(details);
    crashlytics.recordFlutterFatalError(details);
  };
  PlatformDispatcher.instance.onError = (error, stack) {
    crashlytics.recordError(error, stack);
    return true;
  };
  runApp(const MyApp());
}
```

### 按类型处理

```dart
Future<void> fetchData() async {
  try {
    final data = await api.getData();
    // 处理成功
  } on SocketException {
    // 无网络连接 — 显示离线消息
  } on TimeoutException {
    // 请求超时 — 重试选项
  } on FormatException catch (e) {
    // 响应格式错误 — 记录并显示通用错误
    logger.e('API 格式错误', e);
  } catch (e, st) {
    // 意外错误 — 发送到 crashlytics
    crashlytics.recordError(e, st);
  }
}
```

---

## 10. 测试

### BLoC 测试

```dart
import 'package:flutter_test/flutter_test.dart';
import 'package:bloc_test/bloc_test.dart';

bloc_test<AuthBloc, AuthState>(
  'emits [loading, authenticated] when login succeeds',
  build: () {
    when(() => authService.login(any(), any()))
        .thenAnswer((_) async => User(id: '1', name: 'Test'));
    return AuthBloc(authService);
  },
  act: (bloc) => bloc.add(const LoginEvent('test@example.com', 'password')),
  expect: () => [
    const AuthState.loading(),
    isA<AuthState.authenticated>(),
  ],
);
```

### Riverpod 测试

```dart
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

final testContainer = ProviderContainer(
  overrides: [
    userRepositoryProvider.overrideWithValue(FakeUserRepository()),
  ],
);

testWidgets('displays user name', (tester) async {
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: testContainer,
      child: const UserProfileWidget(),
    ),
  );

  expect(find.text('Alice'), findsOneWidget);
});
```

### Widget 测试

```dart
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

testWidgets('counter increments on tap', (tester) async {
  await tester.pumpWidget(
    const ProviderScope(
      child: CounterApp(),
    ),
  );

  expect(find.text('0'), findsOneWidget);
  await tester.tap(find.byIcon(Icons.add));
  await tester.pump();
  expect(find.text('1'), findsOneWidget);
});
```

---

## 相关技能

- `flutter-best-practices` — Flutter 特定最佳实践
- `state-management` — 状态管理模式深入
- `api-design` — API 设计和网络模式