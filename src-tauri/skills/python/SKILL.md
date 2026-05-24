---
name: python-patterns
description: Python 惯用语法、PEP 8 标准、类型提示和最佳实践，用于构建健壮、高效和可维护的 Python 应用程序。
origin: ECC
---

# Python 开发模式

用于构建健壮、高效和可维护应用程序的惯用 Python 模式和最佳实践。

## 激活时机

- 编写新的 Python 代码
- 审查 Python 代码
- 重构现有 Python 代码
- 设计 Python 包/模块

## 核心原则

### 1. 可读性至关重要

Python 优先考虑可读性。代码应该明显且易于理解。

```python
# 好：清晰可读
def get_active_users(users: list[User]) -> list[User]:
    """Return only active users from the provided list."""
    return [user for user in users if user.is_active]


# 坏：巧妙但混乱
def get_active_users(u):
    return [x for x in u if x.a]
```

### 2. 显式优于隐式

避免魔法；要清晰说明你的代码做什么。

```python
# 好：显式配置
import logging

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)

# 坏：隐藏副作用
import some_module
some_module.setup()  # 这是做什么的？
```

### 3. EAFP — 请求原谅比请求许可更容易

Python 优先使用异常处理而非检查条件。

```python
# 好：EAFP 风格
def get_value(dictionary: dict, key: str) -> Any:
    try:
        return dictionary[key]
    except KeyError:
        return default_value

# 坏：LBYL（先看后跳）风格
def get_value(dictionary: dict, key: str) -> Any:
    if key in dictionary:
        return dictionary[key]
    else:
        return default_value
```

## 类型提示

### 基本类型注解

```python
from typing import Optional, List, Dict, Any

def process_user(
    user_id: str,
    data: Dict[str, Any],
    active: bool = True
) -> Optional[User]:
    """Process a user and return the updated User or None."""
    if not active:
        return None
    return User(user_id, data)
```

### 现代类型提示（Python 3.9+）

```python
# Python 3.9+ - 使用内置类型
def process_items(items: list[str]) -> dict[str, int]:
    return {item: len(item) for item in items}

# Python 3.8 及更早 - 使用 typing 模块
from typing import List, Dict

def process_items(items: List[str]) -> Dict[str, int]:
    return {item: len(item) for item in items}
```

### 类型别名和 TypeVar

```python
from typing import TypeVar, Union

# 复杂类型的类型别名
JSON = Union[dict[str, Any], list[Any], str, int, float, bool, None]

def parse_json(data: str) -> JSON:
    return json.loads(data)

# 泛型类型
T = TypeVar('T')

def first(items: list[T]) -> T | None:
    """Return the first item or None if list is empty."""
    return items[0] if items else None
```

### 基于协议的鸭子类型

```python
from typing import Protocol

class Renderable(Protocol):
    def render(self) -> str:
        """Render the object to a string."""

def render_all(items: list[Renderable]) -> str:
    """Render all items that implement the Renderable protocol."""
    return "\n".join(item.render() for item in items)
```

## 错误处理模式

### 特定异常处理

```python
# 好：捕获特定异常
def load_config(path: str) -> Config:
    try:
        with open(path) as f:
            return Config.from_json(f.read())
    except FileNotFoundError as e:
        raise ConfigError(f"Config file not found: {path}") from e
    except json.JSONDecodeError as e:
        raise ConfigError(f"Invalid JSON in config: {path}") from e

# 坏：裸 except
def load_config(path: str) -> Config:
    try:
        with open(path) as f:
            return Config.from_json(f.read())
    except:
        return None  # 静默失败！
```

### 异常链

```python
def process_data(data: str) -> Result:
    try:
        parsed = json.loads(data)
    except json.JSONDecodeError as e:
        # 链异常以保留 traceback
        raise ValueError(f"Failed to parse data: {data}") from e
```

### 自定义异常层次结构

```python
class AppError(Exception):
    """所有应用异常的基类。"""
    pass

class ValidationError(AppError):
    """输入验证失败时抛出。"""
    pass

class NotFoundError(AppError):
    """请求的资源未找到时抛出。"""
    pass

# 使用
def get_user(user_id: str) -> User:
    user = db.find_user(user_id)
    if not user:
        raise NotFoundError(f"User not found: {user_id}")
    return user
```

## 上下文管理器

### 资源管理

```python
# 好：使用上下文管理器
def process_file(path: str) -> str:
    with open(path, 'r') as f:
        return f.read()

# 坏：手动资源管理
def process_file(path: str) -> str:
    f = open(path, 'r')
    try:
        return f.read()
    finally:
        f.close()
```

### 自定义上下文管理器

```python
from contextlib import contextmanager

@contextmanager
def timer(name: str):
    """计时代码块的上下文管理器。"""
    start = time.perf_counter()
    yield
    elapsed = time.perf_counter() - start
    print(f"{name} took {elapsed:.4f} seconds")

# 使用
with timer("data processing"):
    process_large_dataset()
```

### 上下文管理器类

```python
class DatabaseTransaction:
    def __init__(self, connection):
        self.connection = connection

    def __enter__(self):
        self.connection.begin_transaction()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        if exc_type is None:
            self.connection.commit()
        else:
            self.connection.rollback()
        return False  # 不要抑制异常

# 使用
with DatabaseTransaction(conn):
    user = conn.create_user(user_data)
    conn.create_profile(user.id, profile_data)
```

## 推导式和生成器

### 列表推导式

```python
# 好：简单转换的列表推导式
names = [user.name for user in users if user.is_active]

# 坏：手动循环
names = []
for user in users:
    if user.is_active:
        names.append(user.name)

# 复杂推导式应展开
# 坏：太复杂
result = [x * 2 for x in items if x > 0 if x % 2 == 0]

# 好：使用生成器函数
def filter_and_transform(items: Iterable[int]) -> list[int]:
    result = []
    for x in items:
        if x > 0 and x % 2 == 0:
            result.append(x * 2)
    return result
```

### 生成器表达式

```python
# 好：惰性求值的生成器
total = sum(x * x for x in range(1_000_000))

# 坏：创建大型中间列表
total = sum([x * x for x in range(1_000_000)])
```

### 生成器函数

```python
def read_large_file(path: str) -> Iterator[str]:
    """逐行读取大文件。"""
    with open(path) as f:
        for line in f:
            yield line.strip()

# 使用
for line in read_large_file("huge.txt"):
    process(line)
```

## 数据类和命名元组

### 数据类

```python
from dataclasses import dataclass, field
from datetime import datetime

@dataclass
class User:
    """带自动 __init__、__repr__ 和 __eq__ 的用户实体。"""
    id: str
    name: str
    email: str
    created_at: datetime = field(default_factory=datetime.now)
    is_active: bool = True

# 使用
user = User(
    id="123",
    name="Alice",
    email="alice@example.com"
)
```

### 带验证的数据类

```python
@dataclass
class User:
    email: str
    age: int

    def __post_init__(self):
        # 验证 email 格式
        if "@" not in self.email:
            raise ValueError(f"Invalid email: {self.email}")
        # 验证 age 范围
        if self.age < 0 or self.age > 150:
            raise ValueError(f"Invalid age: {self.age}")
```

### 命名元组

```python
from typing import NamedTuple

class Point(NamedTuple):
    """不可变 2D 点。"""
    x: float
    y: float

    def distance(self, other: 'Point') -> float:
        return ((self.x - other.x) ** 2 + (self.y - other.y) ** 2) ** 0.5

# 使用
p1 = Point(0, 0)
p2 = Point(3, 4)
print(p1.distance(p2))  # 5.0
```

## 装饰器

### 函数装饰器

```python
import functools
import time

def timer(func: Callable) -> Callable:
    """计时函数执行的装饰器。"""
    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        start = time.perf_counter()
        result = func(*args, **kwargs)
        elapsed = time.perf_counter() - start
        print(f"{func.__name__} took {elapsed:.4f}s")
        return result
    return wrapper

@timer
def slow_function():
    time.sleep(1)

# slow_function() 打印: slow_function took 1.0012s
```

### 参数化装饰器

```python
def repeat(times: int):
    """重复函数多次的装饰器。"""
    def decorator(func: Callable) -> Callable:
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            results = []
            for _ in range(times):
                results.append(func(*args, **kwargs))
            return results
        return wrapper
    return decorator

@repeat(times=3)
def greet(name: str) -> str:
    return f"Hello, {name}!"

# greet("Alice") 返回 ["Hello, Alice!", "Hello, Alice!", "Hello, Alice!"]
```

### 基于类的装饰器

```python
class CountCalls:
    """计数函数被调用次数的装饰器。"""
    def __init__(self, func: Callable):
        functools.update_wrapper(self, func)
        self.func = func
        self.count = 0

    def __call__(self, *args, **kwargs):
        self.count += 1
        print(f"{self.func.__name__} has been called {self.count} times")
        return self.func(*args, **kwargs)

@CountCalls
def process():
    pass

# 每次调用 process() 打印调用计数
```

## 并发模式

### 用于 I/O 密集型任务的线程

```python
import concurrent.futures
import threading

def fetch_url(url: str) -> str:
    """获取 URL（I/O 密集型操作）。"""
    import urllib.request
    with urllib.request.urlopen(url) as response:
        return response.read().decode()

def fetch_all_urls(urls: list[str]) -> dict[str, str]:
    """使用线程并发获取多个 URL。"""
    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as executor:
        future_to_url = {executor.submit(fetch_url, url): url for url in urls}
        results = {}
        for future in concurrent.futures.as_completed(future_to_url):
            url = future_to_url[future]
            try:
                results[url] = future.result()
            except Exception as e:
                results[url] = f"Error: {e}"
    return results
```

### 用于 CPU 密集型任务的多进程

```python
def process_data(data: list[int]) -> int:
    """CPU 密集型计算。"""
    return sum(x ** 2 for x in data)

def process_all(datasets: list[list[int]]) -> list[int]:
    """使用多进程处理多个数据集。"""
    with concurrent.futures.ProcessPoolExecutor() as executor:
        results = list(executor.map(process_data, datasets))
    return results
```

### 用于并发 I/O 的 Async/Await

```python
import asyncio

async def fetch_async(url: str) -> str:
    """异步获取 URL。"""
    import aiohttp
    async with aiohttp.ClientSession() as session:
        async with session.get(url) as response:
            return await response.text()

async def fetch_all(urls: list[str]) -> dict[str, str]:
    """并发获取多个 URL。"""
    tasks = [fetch_async(url) for url in urls]
    results = await asyncio.gather(*tasks, return_exceptions=True)
    return dict(zip(urls, results))
```

## 包组织

### 标准项目布局

```
myproject/
├── src/
│   └── mypackage/
│       ├── __init__.py
│       ├── main.py
│       ├── api/
│       │   ├── __init__.py
│       │   └── routes.py
│       ├── models/
│       │   ├── __init__.py
│       │   └── user.py
│       └── utils/
│           ├── __init__.py
│           └── helpers.py
├── tests/
│   ├── __init__.py
│   ├── conftest.py
│   ├── test_api.py
│   └── test_models.py
├── pyproject.toml
├── README.md
└── .gitignore
```

### 导入约定

```python
# 好：导入顺序 - 标准库、第三方、本地
import os
import sys
from pathlib import Path

import requests
from fastapi import FastAPI

from mypackage.models import User
from mypackage.utils import format_name

# 好：使用 isort 自动排序导入
# pip install isort
```

### __init__.py 用于包导出

```python
# mypackage/__init__.py
"""mypackage - A sample Python package."""

__version__ = "1.0.0"

# 在包级别导出主要类/函数
from mypackage.models import User, Post
from mypackage.utils import format_name

__all__ = ["User", "Post", "format_name"]
```

## 内存和性能

### 使用 __slots__ 提高内存效率

```python
# 坏：常规类使用 __dict__（更多内存）
class Point:
    def __init__(self, x: float, y: float):
        self.x = x
        self.y = y

# 好：__slots__ 减少内存使用
class Point:
    __slots__ = ['x', 'y']

    def __init__(self, x: float, y: float):
        self.x = x
        self.y = y
```

### 用于大数据集的生成器

```python
# 坏：返回内存中的完整列表
def read_lines(path: str) -> list[str]:
    with open(path) as f:
        return [line.strip() for line in f]

# 好：逐行 yield
def read_lines(path: str) -> Iterator[str]:
    with open(path) as f:
        for line in f:
            yield line.strip()
```

### 避免循环中的字符串连接

```python
# 坏：由于字符串不可变性 O(n²)
result = ""
for item in items:
    result += str(item)

# 好：使用 join 的 O(n)
result = "".join(str(item) for item in items)

# 好：使用 StringIO 构建
from io import StringIO

buffer = StringIO()
for item in items:
    buffer.write(str(item))
result = buffer.getvalue()
```

## Python 工具集成

### 基本命令

```bash
# 代码格式化
black .
isort .

# Linting
ruff check .
pylint mypackage/

# 类型检查
mypy .

# 测试
pytest --cov=mypackage --cov-report=html

# 安全扫描
bandit -r .

# 依赖管理
pip-audit
safety check
```

### pyproject.toml 配置

```toml
[project]
name = "mypackage"
version = "1.0.0"
requires-python = ">=3.9"
dependencies = [
    "requests>=2.31.0",
    "pydantic>=2.0.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.4.0",
    "pytest-cov>=4.1.0",
    "black>=23.0.0",
    "ruff>=0.1.0",
    "mypy>=1.5.0",
]

[tool.black]
line-length = 88
target-version = ['py39']

[tool.ruff]
line-length = 88
select = ["E", "F", "I", "N", "W"]

[tool.mypy]
python_version = "3.9"
warn_return_any = true
warn_unused_configs = true
disallow_untyped_defs = true

[tool.pytest.ini_options]
testpaths = ["tests"]
addopts = "--cov=mypackage --cov-report=term-missing"
```

## 快速参考：Python 惯用语法

| 惯用语法 | 描述 |
|-------|-------------|
| EAFP | 请求原谅比请求许可更容易 |
| 上下文管理器 | 使用 `with` 进行资源管理 |
| 列表推导式 | 用于简单转换 |
| 生成器 | 惰性求值和大数据集 |
| 类型提示 | 注解函数签名 |
| 数据类 | 带自动生成方法的数据容器 |
| `__slots__` | 用于内存优化 |
| f-strings | 用于字符串格式化（Python 3.6+） |
| `pathlib.Path` | 用于路径操作（Python 3.4+） |
| `enumerate` | 用于循环中的索引-元素对 |

## 应避免的反模式

```python
# 坏：可变默认参数
def append_to(item, items=[]):
    items.append(item)
    return items

# 好：使用 None 并创建新列表
def append_to(item, items=None):
    if items is None:
        items = []
    items.append(item)
    return items

# 坏：使用 type() 检查类型
if type(obj) == list:
    process(obj)

# 好：使用 isinstance
if isinstance(obj, list):
    process(obj)

# 坏：使用 == 比较 None
if value == None:
    process()

# 好：使用 is
if value is None:
    process()

# 坏：from module import *
from os.path import *

# 好：显式导入
from os.path import join, exists

# 坏：裸 except
try:
    risky_operation()
except:
    pass

# 好：特定异常
try:
    risky_operation()
except SpecificError as e:
    logger.error(f"Operation failed: {e}")
```

__记住__：Python 代码应该可读、显式并遵循最小意外原则。当有疑问时，优先清晰而非巧妙。

---

---
name: python-testing
description: Python 测试策略，使用 pytest、TDD 方法论、fixtures、mocking、参数化和覆盖率要求。
origin: ECC
---

# Python 测试模式

使用 pytest、TDD 方法论和最佳实践的综合 Python 应用测试策略。

## 激活时机

- 编写新 Python 代码（遵循 TDD：红、绿、重构）
- 为 Python 项目设计测试套件
- 审查 Python 测试覆盖
- 设置测试基础设施

## 核心测试哲学

### 测试驱动开发（TDD）

始终遵循 TDD 循环：

1. **RED**：为期望行为写一个失败的测试
2. **GREEN**：写最小代码使测试通过
3. **REFACTOR**：在保持测试绿色时改进代码

```python
# 步骤 1：写失败的测试（RED）
def test_add_numbers():
    result = add(2, 3)
    assert result == 5

# 步骤 2：写最小实现（GREEN）
def add(a, b):
    return a + b

# 步骤 3：如需要则重构（REFACTOR）
```

### 覆盖率要求

- **目标**：80%+ 代码覆盖率
- **关键路径**：需要 100% 覆盖
- 使用 `pytest --cov` 测量覆盖率

```bash
pytest --cov=mypackage --cov-report=term-missing --cov-report=html
```

## pytest 基础

### 基本测试结构

```python
import pytest

def test_addition():
    """Test basic addition."""
    assert 2 + 2 == 4

def test_string_uppercase():
    """Test string uppercasing."""
    text = "hello"
    assert text.upper() == "HELLO"

def test_list_append():
    """Test list append."""
    items = [1, 2, 3]
    items.append(4)
    assert 4 in items
    assert len(items) == 4
```

### 断言

```python
# 相等
assert result == expected

# 不等
assert result != unexpected

# 真值
assert result  # Truthy
assert not result  # Falsy
assert result is True  # 正好 True
assert result is False  # 正好 False
assert result is None  # 正好 None

# 成员
assert item in collection
assert item not in collection

# 比较
assert result > 0
assert 0 <= result <= 100

# 类型检查
assert isinstance(result, str)

# 异常测试（首选方法）
with pytest.raises(ValueError):
    raise ValueError("error message")

# 检查异常消息
with pytest.raises(ValueError, match="invalid input"):
    raise ValueError("invalid input provided")

# 检查异常属性
with pytest.raises(ValueError) as exc_info:
    raise ValueError("error message")
assert str(exc_info.value) == "error message"
```

## Fixtures

### 基本 Fixture 使用

```python
import pytest

@pytest.fixture
def sample_data():
    """Fixture providing sample data."""
    return {"name": "Alice", "age": 30}

def test_sample_data(sample_data):
    """Test using the fixture."""
    assert sample_data["name"] == "Alice"
    assert sample_data["age"] == 30
```

### 带 Setup/Teardown 的 Fixture

```python
@pytest.fixture
def database():
    """Fixture with setup and teardown."""
    # Setup
    db = Database(":memory:")
    db.create_tables()
    db.insert_test_data()

    yield db  # Provide to test

    # Teardown
    db.close()

def test_database_query(database):
    """Test database operations."""
    result = database.query("SELECT * FROM users")
    assert len(result) > 0
```

### Fixture 作用域

```python
# 函数作用域（默认）- 每个测试运行一次
@pytest.fixture
def temp_file():
    with open("temp.txt", "w") as f:
        yield f
    os.remove("temp.txt")

# 模块作用域 - 每个模块运行一次
@pytest.fixture(scope="module")
def module_db():
    db = Database(":memory:")
    db.create_tables()
    yield db
    db.close()

# 会话作用域 - 每个测试会话运行一次
@pytest.fixture(scope="session")
def shared_resource():
    resource = ExpensiveResource()
    yield resource
    resource.cleanup()
```

### 带参数的 Fixture

```python
@pytest.fixture(params=[1, 2, 3])
def number(request):
    """Parameterized fixture."""
    return request.param

def test_numbers(number):
    """Test runs 3 times, once for each parameter."""
    assert number > 0
```

### 使用多个 Fixtures

```python
@pytest.fixture
def user():
    return User(id=1, name="Alice")

@pytest.fixture
def admin():
    return User(id=2, name="Admin", role="admin")

def test_user_admin_interaction(user, admin):
    """Test using multiple fixtures."""
    assert admin.can_manage(user)
```

### Autouse Fixtures

```python
@pytest.fixture(autouse=True)
def reset_config():
    """Automatically runs before every test."""
    Config.reset()
    yield
    Config.cleanup()

def test_without_fixture_call():
    # reset_config runs automatically
    assert Config.get_setting("debug") is False
```

### Conftest.py 用于共享 Fixtures

```python
# tests/conftest.py
import pytest

@pytest.fixture
def client():
    """Shared fixture for all tests."""
    app = create_app(testing=True)
    with app.test_client() as client:
        yield client

@pytest.fixture
def auth_headers(client):
    """Generate auth headers for API testing."""
    response = client.post("/api/login", json={
        "username": "test",
        "password": "test"
    })
    token = response.json["token"]
    return {"Authorization": f"Bearer {token}"}
```

## 参数化

### 基本参数化

```python
@pytest.mark.parametrize("input,expected", [
    ("hello", "HELLO"),
    ("world", "WORLD"),
    ("PyThOn", "PYTHON"),
])
def test_uppercase(input, expected):
    """Test runs 3 times with different inputs."""
    assert input.upper() == expected
```

### 多个参数

```python
@pytest.mark.parametrize("a,b,expected", [
    (2, 3, 5),
    (0, 0, 0),
    (-1, 1, 0),
    (100, 200, 300),
])
def test_add(a, b, expected):
    """Test addition with multiple inputs."""
    assert add(a, b) == expected
```

### 带 ID 的参数化

```python
@pytest.mark.parametrize("input,expected", [
    ("valid@email.com", True),
    ("invalid", False),
    ("@no-domain.com", False),
], ids=["valid-email", "missing-at", "missing-domain"])
def test_email_validation(input, expected):
    """Test email validation with readable test IDs."""
    assert is_valid_email(input) is expected
```

### 参数化 Fixtures

```python
@pytest.fixture(params=["sqlite", "postgresql", "mysql"])
def db(request):
    """Test against multiple database backends."""
    if request.param == "sqlite":
        return Database(":memory:")
    elif request.param == "postgresql":
        return Database("postgresql://localhost/test")
    elif request.param == "mysql":
        return Database("mysql://localhost/test")

def test_database_operations(db):
    """Test runs 3 times, once for each database."""
    result = db.query("SELECT 1")
    assert result is not None
```

## 标记和测试选择

### 自定义标记

```python
# 标记慢测试
@pytest.mark.slow
def test_slow_operation():
    time.sleep(5)

# 标记集成测试
@pytest.mark.integration
def test_api_integration():
    response = requests.get("https://api.example.com")
    assert response.status_code == 200

# 标记单元测试
@pytest.mark.unit
def test_unit_logic():
    assert calculate(2, 3) == 5
```

### 运行特定测试

```bash
# 仅运行快速测试
pytest -m "not slow"

# 仅运行集成测试
pytest -m integration

# 运行集成或慢测试
pytest -m "integration or slow"

# 运行标记为 unit 但非 slow 的测试
pytest -m "unit and not slow"
```

## Mocking

### 使用 unittest.mock

```python
from unittest.mock import Mock, patch, MagicMock

def test_user_repository():
    # 创建 mock
    mock_db = Mock()
    mock_db.find_user.return_value = User(id=1, name="Alice")

    # 使用 mock
    repo = UserRepository(mock_db)
    user = repo.find_user(1)

    assert user.name == "Alice"
    mock_db.find_user.assert_called_once_with(1)

def test_with_patch():
    @patch('myapp.external_api.fetch')
    def test_external_call(mock_fetch):
        mock_fetch.return_value = {"status": "ok"}

        result = call_external_api()
        assert result["status"] == "ok"
        mock_fetch.assert_called_once()
```

### Fixture 中的 Mock

```python
@pytest.fixture
def mock_db():
    return Mock(spec=Database)

def test_with_mock_db(mock_db):
    mock_db.query.return_value = [{"id": 1, "name": "Alice"}]

    service = UserService(mock_db)
    users = service.get_all()

    assert len(users) == 1
    assert users[0].name == "Alice"
```

## 异步测试

```python
import pytest

@pytest.mark.asyncio
async def test_async_fetch():
    result = await fetch_data()
    assert result is not None

# 或者使用 pytest-asyncio 模式
pytest_plugins = ['pytest_asyncio']
```

## 覆盖率

```bash
# 运行带覆盖的测试
pytest --cov=mypackage --cov-report=term-missing --cov-report=html

# 最低覆盖率检查
pytest --cov=mypackage --cov-fail-under=80

# 仅报告已更改文件的覆盖
pytest --cov=mypackage --cov-report=term-missing --cov-branch
```

## 最佳实践

- **每个测试一个断言** — 保持测试简短专注（可选，但建议）
- **AAA 模式** — Arrange（准备）、Act（行动）、Assert（断言）
- **描述性测试名称** — 测试名称应描述预期行为
- **隔离** — 每个测试独立运行，无隐藏依赖
- **快速** — 单元测试应毫秒级运行
- **确定性** — 无随机失败，无依赖时间
- **Mock 外部依赖** — 不连接真实数据库或 API

## 应避免的反模式

- **不要** 测试实现细节 — 测试行为和输出
- **不要** 在测试间共享可变状态
- **不要** 跳过 `pytest.ini` 或 `pyproject.toml` 配置
- **不要** 使用 `time.sleep` 等待异步操作 — 使用 `pytest.mark.asyncio`
- **不要** 为简单函数写过多测试 — 优先有意义的覆盖
- **不要** 忽略失败的测试 — 立即修复