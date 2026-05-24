---
name: perl-patterns
description: 现代 Perl 5.36+ 惯用语法、最佳实践和约定，用于构建健壮、可维护的 Perl 应用程序。
origin: ECC
---

# 现代 Perl 开发模式

用于构建健壮、可维护应用程序的惯用 Perl 5.36+ 模式和最佳实践。

## 激活时机

- 编写新的 Perl 代码或模块
- 审查 Perl 代码的惯用语法合规性
- 将遗留 Perl 重构为现代标准
- 设计 Perl 模块架构
- 将 pre-5.36 代码迁移到现代 Perl

## 工作原理

将这些模式作为现代 Perl 5.36+ 默认的偏置应用：签名、显式模块、专注错误处理和可测试边界。下面的示例作为起始点复制，然后为你面前实际的应用、依赖栈和部署模型收紧。

## 核心原则

### 1. 使用 `v5.36` 编译指示

单个 `use v5.36` 替换旧样板并启用 strict、warnings 和子例程签名。

```perl
# 好：现代前言
use v5.36;

sub greet($name) {
    say "Hello, $name!";
}

# 坏：遗留样板
use strict;
use warnings;
use feature 'say', 'signatures';
no warnings 'experimental::signatures';

sub greet {
    my ($name) = @_;
    say "Hello, $name!";
}
```

### 2. 子例程签名

使用签名以提高清晰度和自动元数检查。

```perl
use v5.36;

# 好：带默认值的签名
sub connect_db($host, $port = 5432, $timeout = 30) {
    # $host 是必需的，其他有默认值
    return DBI->connect("dbi:Pg:host=$host;port=$port", undef, undef, {
        RaiseError => 1,
        PrintError => 0,
    });
}

# 好：Slurpy 参数用于可变参数
sub log_message($level, @details) {
    say "[$level] " . join(' ', @details);
}

# 坏：手动参数解包
sub connect_db {
    my ($host, $port, $timeout) = @_;
    $port    //= 5432;
    $timeout //= 30;
    # ...
}
```

### 3. 上下文敏感

理解标量 vs 列表上下文 — 一个核心 Perl 概念。

```perl
use v5.36;

my @items = (1, 2, 3, 4, 5);

my @copy  = @items;            # 列表上下文：所有元素
my $count = @items;            # 标量上下文：计数 (5)
say "Items: " . scalar @items; # 强制标量上下文
```

### 4. 后缀解引用

对嵌套结构使用后缀解引用语法以提高可读性。

```perl
use v5.36;

my $data = {
    users => [
        { name => 'Alice', roles => ['admin', 'user'] },
        { name => 'Bob',   roles => ['user'] },
    ],
};

# 好：后缀解引用
my @users = $data->{users}->@*;
my @roles = $data->{users}[0]{roles}->@*;
my %first = $data->{users}[0]->%*;

# 坏：环缀解引用（链中难读）
my @users = @{ $data->{users} };
my @roles = @{ $data->{users}[0]{roles} };
```

### 5. `isa` 操作符（5.32+）

中缀类型检查 — 替换 `blessed($o) && $o->isa('X')`。

```perl
use v5.36;
if ($obj isa 'My::Class') { $obj->do_something }
```

## 错误处理

### eval/die 模式

```perl
use v5.36;

sub parse_config($path) {
    my $content = eval { path($path)->slurp_utf8 };
    die "Config error: $@" if $@;
    return decode_json($content);
}
```

### Try::Tiny（可靠的异常处理）

```perl
use v5.36;
use Try::Tiny;

sub fetch_user($id) {
    my $user = try {
        $db->resultset('User')->find($id)
            // die "User $id not found\n";
    }
    catch {
        warn "Failed to fetch user $id: $_";
        undef;
    };
    return $user;
}
```

### 原生 try/catch（5.40+）

```perl
use v5.40;

sub divide($x, $y) {
    try {
        die "Division by zero" if $y == 0;
        return $x / $y;
    }
    catch ($e) {
        warn "Error: $e";
        return;
    }
}
```

## 使用 Moo 的现代 OO

优先 Moo 用于轻量级、现代 OO。仅在其元协议需要时使用 Moose。

```perl
# 好：Moo 类
package User;
use Moo;
use Types::Standard qw(Str Int ArrayRef);
use namespace::autoclean;

has name  => (is => 'ro', isa => Str, required => 1);
has email => (is => 'ro', isa => Str, required => 1);
has age   => (is => 'ro', isa => Int, default  => sub { 0 });
has roles => (is => 'ro', isa => ArrayRef[Str], default => sub { [] });

sub is_admin($self) {
    return grep { $_ eq 'admin' } $self->roles->@*;
}

sub greet($self) {
    return "Hello, I'm " . $self->name;
}

1;

# 使用
my $user = User->new(
    name  => 'Alice',
    email => 'alice@example.com',
    roles => ['admin', 'user'],
);

# 坏：blessed hashref（无验证，无访问器）
package User;
sub new {
    my ($class, %args) = @_;
    return bless \%args, $class;
}
sub name { return $_[0]->{name} }
1;
```

### Moo 角色

```perl
package Role::Serializable;
use Moo::Role;
use JSON::MaybeXS qw(encode_json);
requires 'TO_HASH';
sub to_json($self) { encode_json($self->TO_HASH) }
1;

package User;
use Moo;
with 'Role::Serializable';
has name  => (is => 'ro', required => 1);
has email => (is => 'ro', required => 1);
sub TO_HASH($self) { { name => $self->name, email => $self->email } }
1;
```

### 原生 `class` 关键字（5.38+，Corinna）

```perl
use v5.38;
use feature 'class';
no warnings 'experimental::class';

class Point {
    field $x :param;
    field $y :param;
    method magnitude() { sqrt($x**2 + $y**2) }
}

my $p = Point->new(x => 3, y => 4);
say $p->magnitude;  # 5
```

## 正则表达式

### 命名捕获和 `/x` 标志

```perl
use v5.36;

# 好：命名捕获和 /x 以提高可读性
my $log_re = qr{
    ^ (?<timestamp> \d{4}-\d{2}-\d{2} \s \d{2}:\d{2}:\d{2} )
    \s+ \[ (?<level> \w+ ) \]
    \s+ (?<message> .+ ) $
}x;

if ($line =~ $log_re) {
    say "Time: $+{timestamp}, Level: $+{level}";
    say "Message: $+{message}";
}

# 坏：位置捕获（难维护）
if ($line =~ /^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+\[(\w+)\]\s+(.+)$/) {
    say "Time: $1, Level: $2";
}
```

### 预编译模式

```perl
use v5.36;

# 好：编译一次，使用多次
my $email_re = qr/^[A-Za-z0-9._%+-]+\@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$/;

sub validate_emails(@emails) {
    return grep { $_ =~ $email_re } @emails;
}
```

## 数据结构

### 引用和安全深度访问

```perl
use v5.36;

# 哈希和数组引用
my $config = {
    database => {
        host => 'localhost',
        port => 5432,
        options => ['utf8', 'sslmode=require'],
    },
};

# 安全深度访问（任何级别缺失返回 undef）
my $port = $config->{database}{port};           # 5432
my $missing = $config->{cache}{host};           # undef，无错误

# 哈希切片
my %subset;
@subset{qw(host port)} = @{$config->{database}}{qw(host port)};

# 数组切片
my @first_two = $config->{database}{options}->@[0, 1];

# 多变量 for 循环（5.36 实验，5.40 稳定）
use feature 'for_list';
no warnings 'experimental::for_list';
for my ($key, $val) (%$config) {
    say "$key => $val";
}
```

## 文件 I/O

### 三参数 Open

```perl
use v5.36;

# 好：三参数 open with autodie（核心模块，消除 'or die'）
use autodie;

sub read_file($path) {
    open my $fh, '<:encoding(UTF-8)', $path;
    local $/;
    my $content = <$fh>;
    close $fh;
    return $content;
}

# 坏：两参数 open（shell 注入风险，参见 perl-security）
open FH, $path;            # 绝不这样做
open FH, "< $path";        # 仍然坏 — 用户数据在模式字符串中
```

### Path::Tiny 用于文件操作

```perl
use v5.36;
use Path::Tiny;

my $file = path('config', 'app.json');
my $content = $file->slurp_utf8;
$file->spew_utf8($new_content);

# 迭代目录
for my $child (path('src')->children(qr/\.pl$/)) {
    say $child->basename;
}
```

## 模块组织

### 标准项目布局

```text
MyApp/
├── lib/
│   └── MyApp/
│       ├── App.pm           # 主模块
│       ├── Config.pm        # 配置
│       ├── DB.pm            # 数据库层
│       └── Util.pm          # 工具
├── bin/
│   └── myapp                # 入口点脚本
├── t/
│   ├── 00-load.t            # 编译测试
│   ├── unit/                # 单元测试
│   └── integration/         # 集成测试
├── cpanfile                 # 依赖
├── Makefile.PL              # 构建系统
└── .perlcriticrc            # Linting 配置
```

### 导出器模式

```perl
package MyApp::Util;
use v5.36;
use Exporter 'import';

our @EXPORT_OK   = qw(trim);
our %EXPORT_TAGS = (all => \@EXPORT_OK);

sub trim($str) { $str =~ s/^\s+|\s+$//gr }

1;
```

## 工具

### perltidy 配置 (.perltidyrc)

```text
-i=4        # 4 空格缩进
-l=100      # 100 字符行长度
-ci=4       # 继续缩进
-ce         # cuddled else
-bar        # 开头大括号在同一行
-nolq       # 不 outdent 长引用字符串
```

### perlcritic 配置 (.perlcriticrc)

```ini
severity = 3
theme = core + pbp + security

[InputOutput::RequireCheckedSyscalls]
functions = :builtins
exclude_functions = say print

[Subroutines::ProhibitExplicitReturnUndef]
severity = 4

[ValuesAndExpressions::ProhibitMagicNumbers]
allowed_values = 0 1 2 -1
```

### 依赖管理（cpanfile + carton）

```bash
cpanm App::cpanminus Carton   # 安装工具
carton install                 # 从 cpanfile 安装依赖
carton exec -- perl bin/myapp  # 使用本地依赖运行
```

```perl
# cpanfile
requires 'Moo', '>= 2.005';
requires 'Path::Tiny';
requires 'JSON::MaybeXS';
requires 'Try::Tiny';

on test => sub {
    requires 'Test2::V0';
    requires 'Test::MockModule';
};
```

## 快速参考：现代 Perl 惯用语法

| 遗留模式 | 现代替换 |
|---|---|
| `use strict; use warnings;` | `use v5.36;` |
| `my ($x, $y) = @_;` | `sub foo($x, $y) { ... }` |
| `@{ $ref }` | `$ref->@*` |
| `%{ $ref }` | `$ref->%*` |
| `open FH, "< $file"` | `open my $fh, '<:encoding(UTF-8)', $file` |
| `blessed hashref` | 带类型的 `Moo` 类 |
| `$1, $2, $3` | `$+{name}`（命名捕获） |
| `eval { }; if ($@)` | `Try::Tiny` 或原生 `try/catch`（5.40+） |
| `BEGIN { require Exporter; }` | `use Exporter 'import';` |
| 手动文件操作 | `Path::Tiny` |
| `blessed($o) && $o->isa('X')` | `$o isa 'X'`（5.32+） |
| `builtin::true / false` | `use builtin 'true', 'false';`（5.36+，实验性） |

## 反模式

```perl
# 1. 两参数 open（安全风险）
open FH, $filename;                     # 绝不

# 2. 间接对象语法（解析歧义）
my $obj = new Foo(bar => 1);            # 坏
my $obj = Foo->new(bar => 1);           # 好

# 3. 过度依赖 $_
map { process($_) } grep { validate($_) } @items;  # 难跟随
my @valid = grep { validate($_) } @items;           # 更好：分解
my @results = map { process($_) } @valid;

# 4. 禁用 strict refs
no strict 'refs';                        # 几乎总是错
${"My::Package::$var"} = $value;         # 改用哈希

# 5. 全局变量作为配置
our $TIMEOUT = 30;                       # 坏：可变全局
use constant TIMEOUT => 30;              # 更好：常量
# 最好：带默认值的 Moo 属性

# 6. 字符串 eval 加载模块
eval "require $module";                  # 坏：代码注入风险
eval "use $module";                      # 坏
use Module::Runtime 'require_module';    # 好：安全模块加载
require_module($module);
```

**记住**：现代 Perl 是清洁、可读和安全的。让 `use v5.36` 处理样板，使用 Moo 处理对象，优先使用 CPAN 的经过实战测试的模块而非手写解决方案。

---

---
name: perl-testing
description: Perl 测试模式，使用 Test2::V0、Test::More、prove 运行器、mock、Devel::Cover 覆盖率和 TDD 方法论。
origin: ECC
---

# Perl 测试模式

使用 Test2::V0、Test::More、prove 和 TDD 方法论的综合 Perl 应用测试策略。

## 激活时机

- 编写新 Perl 代码（遵循 TDD：红、绿、重构）
- 为 Perl 模块或应用设计测试套件
- 审查 Perl 测试覆盖
- 设置 Perl 测试基础设施
- 将测试从 Test::More 迁移到 Test2::V0
- 调试失败的 Perl 测试

## TDD 工作流

始终遵循 RED-GREEN-REFACTOR 循环。

```perl
# 步骤 1：RED — 写一个失败的测试
# t/unit/calculator.t
use v5.36;
use Test2::V0;

use lib 'lib';
use Calculator;

subtest 'addition' => sub {
    my $calc = Calculator->new;
    is($calc->add(2, 3), 5, 'adds two numbers');
    is($calc->add(-1, 1), 0, 'handles negatives');
};

done_testing;

# 步骤 2：GREEN — 写最小实现
# lib/Calculator.pm
package Calculator;
use v5.36;
use Moo;

sub add($self, $a, $b) {
    return $a + $b;
}

1;

# 步骤 3：REFACTOR — 在测试保持绿色时改进
# 运行：prove -lv t/unit/calculator.t
```

## Test::More 基础

标准 Perl 测试模块 — 广泛使用，随核心一起发货。

### 基本断言

```perl
use v5.36;
use Test::More;

# 预先计划或使用 done_testing
# plan tests => 5;  # 固定计划（可选）

# 相等
is($result, 42, 'returns correct value');
isnt($result, 0, 'not zero');

# 布尔
ok($user->is_active, 'user is active');
ok(!$user->is_banned, 'user is not banned');

# 深度比较
is_deeply(
    $got,
    { name => 'Alice', roles => ['admin'] },
    'returns expected structure'
);

# 模式匹配
like($error, qr/not found/i, 'error mentions not found');
unlike($output, qr/password/, 'output hides password');

# 类型检查
isa_ok($obj, 'MyApp::User');
can_ok($obj, 'save', 'delete');

done_testing;
```

### SKIP 和 TODO

```perl
use v5.36;
use Test::More;

# 条件跳过测试
SKIP: {
    skip 'No database configured', 2 unless $ENV{TEST_DB};

    my $db = connect_db();
    ok($db->ping, 'database is reachable');
    is($db->version, '15', 'correct PostgreSQL version');
}

# 标记预期失败
TODO: {
    local $TODO = 'Caching not yet implemented';
    is($cache->get('key'), 'value', 'cache returns value');
}

done_testing;
```

## Test2::V0 现代框架

Test2::V0 是 Test::More 的现代替代 — 更丰富的断言、更好的诊断和可扩展。

### 为什么用 Test2？

- 通过哈希/数组构建器优越的深度比较
- 失败时更好的诊断输出
- 更清洁作用域的子测试
- 通过 Test2::Tools::* 插件可扩展
- 与 Test::More 测试向后兼容

### 使用构建器的深度比较

```perl
use v5.36;
use Test2::V0;

# 哈希构建器 — 检查部分结构
is(
    $user->to_hash,
    hash {
        field name  => 'Alice';
        field email => match(qr/\@example\.com$/);
        field age   => validator(sub { $_ >= 18 });
        # 忽略其他字段
        etc();
    },
    'user has expected fields'
);

# 数组构建器
is(
    $result,
    array {
        item 'first';
        item match(qr/^second/);
        item DNE();  # 不存在 — 验证无额外项
    },
    'result matches expected list'
);

# Bag — 顺序无关比较
is(
    $tags,
    bag {
        item 'perl';
        item 'testing';
        item 'tdd';
    },
    'has all required tags regardless of order'
);
```

### 子测试

```perl
use v5.36;
use Test2::V0;

subtest 'User creation' => sub {
    my $user = User->new(name => 'Alice', email => 'alice@example.com');
    ok($user, 'user object created');
    is($user->name, 'Alice', 'name is set');
    is($user->email, 'alice@example.com', 'email is set');
};

subtest 'User validation' => sub {
    my $warnings = warns {
        User->new(name => '', email => 'bad');
    };
    ok($warnings, 'warns on invalid data');
};

done_testing;
```

### Test2 异常测试

```perl
use v5.36;
use Test2::V0;

# 测试代码死亡
like(
    dies { divide(10, 0) },
    qr/Division by zero/,
    'dies on division by zero'
);

# 测试代码存活
ok(lives { divide(10, 2) }, 'division succeeds') or note($@);

# 组合模式
subtest 'error handling' => sub {
    ok(lives { parse_config('valid.json') }, 'valid config parses');
    like(
        dies { parse_config('missing.json') },
        qr/Cannot open/,
        'missing file dies with message'
    );
};

done_testing;
```

## 测试组织和 prove

### 目录结构

```text
t/
├── 00-load.t              # 验证模块编译
├── 01-basic.t             # 核心功能
├── unit/
│   ├── config.t           # 按模块的单元测试
│   ├── user.t
│   └── util.t
├── integration/
│   ├── database.t
│   └── api.t
├── lib/
│   └── TestHelper.pm      # 共享测试工具
└── fixtures/
    ├── config.json        # 测试数据文件
    └── users.csv
```

### prove 命令

```bash
# 运行所有测试
prove -l t/

# 详细输出
prove -lv t/

# 运行特定测试
prove -lv t/unit/user.t

# 递归搜索
prove -lr t/

# 并行执行（8 个作业）
prove -lr -j8 t/

# 仅运行上次运行的失败测试
prove -l --state=failed t/

# 带计时器的着色输出
prove -l --color --timer t/

# CI 兼容 TAP 输出
prove -l --formatter TAP::Formatter::JUnit t/ > results.xml
```

### .proverc 配置

```text
-l
--color
--timer
-r
-j4
--state=save
```

## Fixtures 和 Setup/Teardown

### 子测试隔离

```perl
use v5.36;
use Test2::V0;
use File::Temp qw(tempdir);
use Path::Tiny;

subtest 'file processing' => sub {
    # Setup
    my $dir = tempdir(CLEANUP => 1);
    my $file = path($dir, 'input.txt');
    $file->spew_utf8("line1\nline2\nline3\n");

    # Test
    my $result = process_file("$file");
    is($result->{line_count}, 3, 'counts lines');

    # Teardown 自动发生（CLEANUP => 1）
};
```

### 共享测试辅助

在 `t/lib/TestHelper.pm` 中放置可重用辅助，通过 `use lib 't/lib'` 加载。通过 `Exporter` 导出工厂函数如 `create_test_db()`、`create_temp_dir()` 和 `fixture_path()`。

## Mocking

### Test::MockModule

```perl
use v5.36;
use Test2::V0;
use Test::MockModule;

subtest 'mock external API' => sub {
    my $mock = Test::MockModule->new('MyApp::API');

    # 好：Mock 返回受控数据
    $mock->mock(fetch_user => sub ($self, $id) {
        return { id => $id, name => 'Mock User', email => 'mock@test.com' };
    });

    my $api = MyApp::API->new;
    my $user = $api->fetch_user(42);
    is($user->{name}, 'Mock User', 'returns mocked user');

    # 验证调用计数
    my $call_count = 0;
    $mock->mock(fetch_user => sub { $call_count++; return {} });
    $api->fetch_user(1);
    $api->fetch_user(2);
    is($call_count, 2, 'fetch_user called twice');

    # $mock 超出作用域时自动恢复 mock
};

# 坏：在没有恢复的情况下猴子补丁
# *MyApp::API::fetch_user = sub { ... };  # 绝不 — 泄漏到测试间
```

对于轻量级 mock 对象，使用 `Test::MockObject` 创建可注入测试双精度，带 `->mock()` 并用 `->called_ok()` 验证调用。

## 使用 Devel::Cover 的覆盖

### 运行覆盖

```bash
# 基本覆盖报告
cover -test

# 或逐步
perl -MDevel::Cover -Ilib t/unit/user.t
cover

# HTML 报告
cover -report html
open cover_db/coverage.html

# 特定阈值
cover -test -report text | grep 'Total'

# CI 友好：低于阈值失败
cover -test && cover -report text -select '^lib/' \
  | perl -ne 'if (/Total.*?(\d+\.\d+)/) { exit 1 if $1 < 80 }'
```

### 集成测试

对数据库测试使用内存 SQLite，对 API 测试 mock HTTP::Tiny。

```perl
use v5.36;
use Test2::V0;
use DBI;

subtest 'database integration' => sub {
    my $dbh = DBI->connect('dbi:SQLite:dbname=:memory:', '', '', {
        RaiseError => 1,
    });
    $dbh->do('CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)');

    $dbh->prepare('INSERT INTO users (name) VALUES (?)')->execute('Alice');
    my $row = $dbh->selectrow_hashref('SELECT * FROM users WHERE name = ?', undef, 'Alice');
    is($row->{name}, 'Alice', 'inserted and retrieved user');
};

done_testing;
```

## 最佳实践

### 做

- **遵循 TDD**：先写测试再实现（红-绿-重构）
- **使用 Test2::V0**：现代断言，更好诊断
- **使用子测试**：分组相关断言，隔离状态
- **Mock 外部依赖**：网络、数据库、文件系统
- **使用 `prove -l`**：始终在 @INC 中包含 lib/
- **清晰命名测试**：`'user login with invalid password fails'`
- **测试边界情况**：空字符串、undef、零、边界值
- **目标 80%+ 覆盖**：专注业务逻辑路径
- **保持测试快速**：Mock I/O，使用内存数据库

### 不要做

- **不要测试实现**：测试行为和输出，而非内部
- **不要在子测试间共享状态**：每个子测试应独立
- **不要跳过 `done_testing`**：确保所有计划测试运行
- **不要过度 mock**：仅 mock 边界，不 mock 被测代码
- **不要为新项目使用 `Test::More`**：优先 Test2::V0
- **不要忽略测试失败**：合并前所有测试必须通过
- **不要测试 CPAN 模块**：信任库正确工作
- **不要写脆弱测试**：避免过度特定的字符串匹配

## 快速参考

| 任务 | 命令 / 模式 |
|---|---|
| 运行所有测试 | `prove -lr t/` |
| 详细运行一个测试 | `prove -lv t/unit/user.t` |
| 并行测试运行 | `prove -lr -j8 t/` |
| 覆盖报告 | `cover -test && cover -report html` |
| 测试相等 | `is($got, $expected, 'label')` |
| 深度比较 | `is($got, hash { field k => 'v'; etc() }, 'label')` |
| 测试异常 | `like(dies { ... }, qr/msg/, 'label')` |
| 测试无异常 | `ok(lives { ... }, 'label')` |
| Mock 方法 | `Test::MockModule->new('Pkg')->mock(m => sub { ... })` |
| 跳过测试 | `SKIP: { skip 'reason', $count unless $cond; ... }` |
| TODO 测试 | `TODO: { local $TODO = 'reason'; ... }` |

## 常见陷阱

### 忘记 `done_testing`

```perl
# 坏：测试文件运行但不验证所有测试执行
use Test2::V0;
is(1, 1, 'works');
# 缺少 done_testing — 如果测试代码被跳过则静默 bug

# 好：始终以 done_testing 结束
use Test2::V0;
is(1, 1, 'works');
done_testing;
```

### 缺少 `-l` 标志

```bash
# 坏：找不到 lib/ 中的模块
prove t/unit/user.t
# Cannot locate MyApp/User.pm in @INC

# 好：在 @INC 中包含 lib/
prove -l t/unit/user.t
```

### 过度 Mock

Mock *依赖*，而非被测代码。如果测试仅验证 mock 返回你告诉它的值，它什么也没测。

### 测试污染

在子测试内使用 `my` 变量 — 绝不 `our` — 防止状态在测试间泄漏。

**记住**：测试是你的安全网。保持它们快速、专注和独立。使用 Test2::V0 用于新项目，prove 用于运行，Devel::Cover 用于问责。

---

---
name: perl-security
description: 综合 Perl 安全覆盖污染模式、输入验证、安全进程执行、DBI 参数化查询、Web 安全（XSS/SQLi/CSRF）和 perlcritic 安全策略。
origin: ECC
---

# Perl 安全模式

涵盖输入验证、注入预防和安全编码实践的综合 Perl 应用安全指南。

## 激活时机

- 在 Perl 应用中处理用户输入
- 构建 Perl Web 应用（CGI、Mojolicious、Dancer2、Catalyst）
- 审查 Perl 代码的安全漏洞
- 执行带用户提供路径的文件操作
- 从 Perl 执行系统命令
- 编写 DBI 数据库查询

## 工作原理

从污染感知输入边界开始，然后向外移动：验证和取消污染输入、保持文件系统和进程执行受限、在各处使用参数化 DBI 查询。以下示例显示此技能期望你在发货触碰用户输入、shell 或网络的 Perl 代码前应用的 安全默认设置。

## 污染模式

Perl 的污染模式（`-T`）跟踪来自外部来源的数据并防止其在未经显式验证的情况下用于不安全操作。

### 启用污染模式

```perl
#!/usr/bin/perl -T
use v5.36;

# 污染：来自程序外部的任何东西
my $input    = $ARGV[0];        # 污染
my $env_path = $ENV{PATH};      # 污染
my $form     = <STDIN>;         # 污染
my $query    = $ENV{QUERY_STRING}; # 污染

# 早期清理 PATH（污染模式中必需）
$ENV{PATH} = '/usr/local/bin:/usr/bin:/bin';
delete @ENV{qw(IFS CDPATH ENV BASH_ENV)};
```

### 取消污染模式

```perl
use v5.36;

# 好：使用特定 regex 验证和取消污染
sub untaint_username($input) {
    if ($input =~ /^([a-zA-Z0-9_]{3,30})$/) {
        return $1;  # $1 是非污染的
    }
    die "Invalid username: must be 3-30 alphanumeric characters\n";
}

# 好：验证和取消污染文件路径
sub untaint_filename($input) {
    if ($input =~ m{^([a-zA-Z0-9._-]+)$}) {
        return $1;
    }
    die "Invalid filename: contains unsafe characters\n";
}

# 坏：过于宽松的取消污染（破坏目的）
sub bad_untaint($input) {
    $input =~ /^(.*)$/s;
    return $1;  # 接受任何东西 — 无意义
}
```

## 输入验证

### 允许列表优于拒绝列表

```perl
use v5.36;

# 好：允许列表 — 精确定义允许的内容
sub validate_sort_field($field) {
    my %allowed = map { $_ => 1 } qw(name email created_at updated_at);
    die "Invalid sort field: $field\n" unless $allowed{$field};
    return $field;
}

# 好：用特定模式验证
sub validate_email($email) {
    if ($email =~ /^([a-zA-Z0-9._%+-]+\@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,})$/) {
        return $1;
    }
    die "Invalid email address\n";
}

sub validate_integer($input) {
    if ($input =~ /^(-?\d{1,10})$/) {
        return $1 + 0;  # 强制为数字
    }
    die "Invalid integer\n";
}

# 坏：拒绝列表 — 总是 incomplete
sub bad_validate($input) {
    die "Invalid" if $input =~ /[<>"';&|]/;  # 遗漏编码攻击
    return $input;
}
```

### 长度约束

```perl
use v5.36;

sub validate_comment($text) {
    die "Comment is required\n"        unless length($text) > 0;
    die "Comment exceeds 10000 chars\n" if length($text) > 10_000;
    return $text;
}
```

## 安全正则表达式

### ReDoS 预防

灾难性回溯发生在重叠模式上的嵌套量词。

```perl
use v5.36;

# 坏：易受 ReDoS 攻击（指数回溯）
my $bad_re = qr/^(a+)+$/;           # 嵌套量词
my $bad_re2 = qr/^([a-zA-Z]+)*$/;   # 类上嵌套量词
my $bad_re3 = qr/^(.*?,){10,}$/;    # 重复贪婪/惰性组合

# 好：无嵌套重写
my $good_re = qr/^a+$/;             # 单数量词
my $good_re2 = qr/^[a-zA-Z]+$/;     # 类上单数量词

# 好：使用占有量词或原子组防止回溯
my $safe_re = qr/^[a-zA-Z]++$/;             # 占有（5.10+）
my $safe_re2 = qr/^(?>a+)$/;                # 原子组

# 好：在不受信任模式上强制超时
use POSIX qw(alarm);
sub safe_match($string, $pattern, $timeout = 2) {
    my $matched;
    eval {
        local $SIG{ALRM} = sub { die "Regex timeout\n" };
        alarm($timeout);
        $matched = $string =~ $pattern;
        alarm(0);
    };
    alarm(0);
    die $@ if $@;
    return $matched;
}
```

## 安全文件操作

### 三参数 Open

```perl
use v5.36;

# 好：三参数 open、词法文件句柄、检查返回值
sub read_file($path) {
    open my $fh, '<:encoding(UTF-8)', $path
        or die "Cannot open '$path': $!\n";
    local $/;
    my $content = <$fh>;
    close $fh;
    return $content;
}

# 坏：两参数 open with 用户数据（命令注入）
sub bad_read($path) {
    open my $fh, $path;        # 如果 $path = "|rm -rf /"，运行命令！
    open my $fh, "< $path";   # shell 元字符注入
}
```

### TOCTOU 预防和路径遍历

```perl
use v5.36;
use Fcntl qw(:DEFAULT :flock);
use File::Spec;
use Cwd qw(realpath);

# 原子文件创建
sub create_file_safe($path) {
    sysopen(my $fh, $path, O_WRONLY | O_CREAT | O_EXCL, 0600)
        or die "Cannot create '$path': $!\n";
    return $fh;
}

# 验证路径保持在允许目录内
sub safe_path($base_dir, $user_path) {
    my $real = realpath(File::Spec->catfile($base_dir, $user_path))
        // die "Path does not exist\n";
    my $base_real = realpath($base_dir)
        // die "Base dir does not exist\n";
    die "Path traversal blocked\n" unless $real =~ /^\Q$base_real\E(?:\/|\z)/;
    return $real;
}
```

使用 `File::Temp` 用于临时文件（`tempfile(UNLINK => 1)`）和 `flock(LOCK_EX)` 防止竞态条件。

## 安全进程执行

### 列表形式 system 和 exec

```perl
use v5.36;

# 好：列表形式 — 无 shell 插值
sub run_command(@cmd) {
    system(@cmd) == 0
        or die "Command failed: @cmd\n";
}

run_command('grep', '-r', $user_pattern, '/var/log/app/');

# 好：使用 IPC::Run3 安全捕获输出
use IPC::Run3;
sub capture_output(@cmd) {
    my ($stdout, $stderr);
    run3(\@cmd, \undef, \$stdout, \$stderr);
    if ($?) {
        die "Command failed (exit $?): $stderr\n";
    }
    return $stdout;
}

# 坏：字符串形式 — shell 注入！
sub bad_search($pattern) {
    system("grep -r '$pattern' /var/log/app/");  # 如果 $pattern = "'; rm -rf / #"
}

# 坏：带插值的反引号
my $output = `ls $user_dir`;   # shell 注入风险
```

也使用 `Capture::Tiny` 用于安全捕获外部命令的 stdout/stderr。

## SQL 注入预防

### DBI 占位符

```perl
use v5.36;
use DBI;

my $dbh = DBI->connect($dsn, $user, $pass, {
    RaiseError => 1,
    PrintError => 0,
    AutoCommit => 1,
});

# 好：参数化查询 — 始终使用占位符
sub find_user($dbh, $email) {
    my $sth = $dbh->prepare('SELECT * FROM users WHERE email = ?');
    $sth->execute($email);
    return $sth->fetchrow_hashref;
}

sub search_users($dbh, $name, $status) {
    my $sth = $dbh->prepare(
        'SELECT * FROM users WHERE name LIKE ? AND status = ? ORDER BY name'
    );
    $sth->execute("%$name%", $status);
    return $sth->fetchall_arrayref({});
}

# 坏：SQL 中的字符串插值（SQLi 漏洞！）
sub bad_find($dbh, $email) {
    my $sth = $dbh->prepare("SELECT * FROM users WHERE email = '$email'");
    # 如果 $email = "' OR 1=1 --"，返回所有用户
    $sth->execute;
    return $sth->fetchrow_hashref;
}
```

### 动态列允许列表

```perl
use v5.36;

# 好：针对允许列表验证列名
sub order_by($dbh, $column, $direction) {
    my %allowed_cols = map { $_ => 1 } qw(name email created_at);
    my %allowed_dirs = map { $_ => 1 } qw(ASC DESC);

    die "Invalid column: $column\n"    unless $allowed_cols{$column};
    die "Invalid direction: $direction\n" unless $allowed_dirs{uc $direction};

    my $sth = $dbh->prepare("SELECT * FROM users ORDER BY $column $direction");
    $sth->execute;
    return $sth->fetchall_arrayref({});
}

# 坏：直接插值用户选择的列
sub bad_order($dbh, $column) {
    $dbh->prepare("SELECT * FROM users ORDER BY $column");  # SQLi！
}
```

### DBIx::Class（ORM 安全）

```perl
use v5.36;

# DBIx::Class 生成安全参数化查询
my @users = $schema->resultset('User')->search({
    status => 'active',
    email  => { -like => '%@example.com' },
}, {
    order_by => { -asc => 'name' },
    rows     => 50,
});
```

## Web 安全

### XSS 预防

```perl
use v5.36;
use HTML::Entities qw(encode_entities);
use URI::Escape qw(uri_escape_utf8);

# 好：为 HTML 上下文编码输出
sub safe_html($user_input) {
    return encode_entities($user_input);
}

# 好：为 URL 上下文编码
sub safe_url_param($value) {
    return uri_escape_utf8($value);
}

# 好：为 JSON 上下文编码
use JSON::MaybeXS qw(encode_json);
sub safe_json($data) {
    return encode_json($data);  # 处理转义
}

# 模板自动转义（Mojolicious）
# <%= $user_input %>   — 自动转义（安全）
# <%== $raw_html %>    — 原始输出（危险，仅用于可信内容）

# 模板自动转义（Template Toolkit）
# [% user_input | html %]  — 显式 HTML 编码

# 坏：HTML 中原始输出
sub bad_html($input) {
    print "<div>$input</div>";  # XSS 如果 $input 包含 <script>
}
```

### CSRF 保护

```perl
use v5.36;
use Crypt::URandom qw(urandom);
use MIME::Base64 qw(encode_base64url);

sub generate_csrf_token() {
    return encode_base64url(urandom(32));
}
```

验证 token 时使用常量时间比较。大多数 Web 框架（Mojolicious、Dancer2、Catalyst）提供内置 CSRF 保护 — 优先使用这些而非手写解决方案。

### 会话和头安全

```perl
use v5.36;

# Mojolicious 会话 + 头
$app->secrets(['long-random-secret-rotated-regularly']);
$app->sessions->secure(1);          # 仅 HTTPS
$app->sessions->samesite('Lax');

$app->hook(after_dispatch => sub ($c) {
    $c->res->headers->header('X-Content-Type-Options' => 'nosniff');
    $c->res->headers->header('X-Frame-Options'        => 'DENY');
    $c->res->headers->header('Content-Security-Policy' => "default-src 'self'");
    $c->res->headers->header('Strict-Transport-Security' => 'max-age=31536000; includeSubDomains');
});
```

## 输出编码

始终为其上下文编码输出：`HTML::Entities::encode_entities()` 用于 HTML，`URI::Escape::uri_escape_utf8()` 用于 URLs，`JSON::MaybeXS::encode_json()` 用于 JSON。

## CPAN 模块安全

- **在 cpanfile 中固定版本**：`requires 'DBI', '== 1.643';`
- **优先维护的模块**：检查 MetaCPAN 近期发布
- **最小化依赖**：每个依赖都是一个攻击面

## 安全工具

### perlcritic 安全策略

```ini
# .perlcriticrc — 安全聚焦配置
severity = 3
theme = security + core

# 要求三参数 open
[InputOutput::RequireThreeArgOpen]
severity = 5

# 要求检查系统调用
[InputOutput::RequireCheckedSyscalls]
functions = :builtins
severity = 4

# 禁止字符串 eval
[BuiltinFunctions::ProhibitStringyEval]
severity = 5

# 禁止反引号操作符
[InputOutput::ProhibitBacktickOperators]
severity = 4

# CGI 中要求污染检查
[Modules::RequireTaintChecking]
severity = 5

# 禁止两参数 open
[InputOutput::ProhibitTwoArgOpen]
severity = 5

# 禁止裸词文件句柄
[InputOutput::ProhibitBarewordFileHandles]
severity = 5
```

### 运行 perlcritic

```bash
# 检查一个文件
perlcritic --severity 3 --theme security lib/MyApp/Handler.pm

# 检查整个项目
perlcritic --severity 3 --theme security lib/

# CI 集成
perlcritic --severity 4 --theme security --quiet lib/ || exit 1
```

## 快速安全检查清单

| 检查 | 验证什么 |
|---|---|
| 污染模式 | CGI/Web 脚本上的 `-T` 标志 |
| 输入验证 | 允许列表模式、长度限制 |
| 文件操作 | 三参数 open、路径遍历检查 |
| 进程执行 | 列表形式 system，无 shell 插值 |
| SQL 查询 | DBI 占位符，绝不插值 |
| HTML 输出 | `encode_entities()`、模板自动转义 |
| CSRF token | 生成，在状态改变请求时验证 |
| 会话配置 | 安全、HttpOnly、SameSite cookies |
| HTTP 头 | CSP、X-Frame-Options、HSTS |
| 依赖 | 固定版本、审计模块 |
| Regex 安全 | 无嵌套量词、锚定模式 |
| 错误消息 | 无堆栈跟踪或路径泄露给用户 |

## 反模式

```perl
# 1. 带用户数据的两参数 open（命令注入）
open my $fh, $user_input;               # 关键漏洞

# 2. 字符串形式 system（shell 注入）
system("convert $user_file output.png"); # 关键漏洞

# 3. SQL 字符串插值
$dbh->do("DELETE FROM users WHERE id = $id");  # SQLi

# 4. 带用户输入的 eval（代码注入）
eval $user_code;                         # 远程代码执行

# 5. 不清理信任 $ENV
my $path = $ENV{UPLOAD_DIR};             # 可能被操纵
system("ls $path");                      # 双重漏洞

# 6. 无验证禁用污染
($input) = $input =~ /(.*)/s;           # 惰性取消污染 — 破坏目的

# 7. HTML 中原始用户数据
print "<div>Welcome, $username!</div>";  # XSS

# 8. 未验证的重定向
print $cgi->redirect($user_url);         # 开放重定向
```