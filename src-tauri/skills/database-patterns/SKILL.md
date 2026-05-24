---
name: postgres-patterns
description: PostgreSQL 数据库模式，涵盖查询优化、schema 设计、索引和安全。基于 Supabase 最佳实践。
origin: ECC
---

# PostgreSQL 模式

PostgreSQL 最佳实践快速参考。详细指导，使用 `database-reviewer` 智能体。

## 激活时机

- 编写 SQL 查询或迁移
- 设计数据库 schema
- 排查慢查询
- 实现行级安全
- 设置连接池

## 快速参考

### 索引速查表

| 查询模式 | 索引类型 | 示例 |
|--------------|------------|---------|
| `WHERE col = value` | B-tree (默认) | `CREATE INDEX idx ON t (col)` |
| `WHERE col > value` | B-tree | `CREATE INDEX idx ON t (col)` |
| `WHERE a = x AND b > y` | 复合索引 | `CREATE INDEX idx ON t (a, b)` |
| `WHERE jsonb @> '{}'` | GIN | `CREATE INDEX idx ON t USING gin (col)` |
| `WHERE tsv @@ query` | GIN | `CREATE INDEX idx ON t USING gin (col)` |
| 时间序列范围 | BRIN | `CREATE INDEX idx ON t USING brin (col)` |

### 数据类型快速参考

| 使用场景 | 正确类型 | 避免 |
|----------|-------------|-------|
| ID | `bigint` | `int`、随机 UUID |
| 字符串 | `text` | `varchar(255)` |
| 时间戳 | `timestamptz` | `timestamp` |
| 金额 | `numeric(10,2)` | `float` |
| 标志 | `boolean` | `varchar`、`int` |

### 常见模式

**复合索引顺序：**
```sql
-- 先等值列，再范围列
CREATE INDEX idx ON orders (status, created_at);
-- 适用于: WHERE status = 'pending' AND created_at > '2024-01-01'
```

**覆盖索引：**
```sql
CREATE INDEX idx ON users (email) INCLUDE (name, created_at);
-- 避免 SELECT email, name, created_at 的表查找
```

**部分索引：**
```sql
CREATE INDEX idx ON users (email) WHERE deleted_at IS NULL;
-- 更小的索引，仅包含活跃用户
```

**RLS 策略（优化）：**
```sql
CREATE POLICY policy ON orders
  USING ((SELECT auth.uid()) = user_id);  -- 用 SELECT 包装！
```

**UPSERT：**
```sql
INSERT INTO settings (user_id, key, value)
VALUES (123, 'theme', 'dark')
ON CONFLICT (user_id, key)
DO UPDATE SET value = EXCLUDED.value;
```

**游标分页：**
```sql
SELECT * FROM products WHERE id > $last_id ORDER BY id LIMIT 20;
-- O(1) vs OFFSET 的 O(n)
```

**队列处理：**
```sql
UPDATE jobs SET status = 'processing'
WHERE id = (
  SELECT id FROM jobs WHERE status = 'pending'
  ORDER BY created_at LIMIT 1
  FOR UPDATE SKIP LOCKED
) RETURNING *;
```

### 反模式检测

```sql
-- 查找未索引的外键
SELECT conrelid::regclass, a.attname
FROM pg_constraint c
JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = ANY(c.conkey)
WHERE c.contype = 'f'
  AND NOT EXISTS (
    SELECT 1 FROM pg_index i
    WHERE i.indrelid = c.conrelid AND a.attnum = ANY(i.indkey)
  );

-- 查找慢查询
SELECT query, mean_exec_time, calls
FROM pg_stat_statements
WHERE mean_exec_time > 100
ORDER BY mean_exec_time DESC;

-- 检查表膨胀
SELECT relname, n_dead_tup, last_vacuum
FROM pg_stat_user_tables
WHERE n_dead_tup > 1000
ORDER BY n_dead_tup DESC;
```

### 配置模板

```sql
-- 连接限制（根据 RAM 调整）
ALTER SYSTEM SET max_connections = 100;
ALTER SYSTEM SET work_mem = '8MB';

-- 超时
ALTER SYSTEM SET idle_in_transaction_session_timeout = '30s';
ALTER SYSTEM SET statement_timeout = '30s';

-- 监控
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- 安全默认值
REVOKE ALL ON SCHEMA public FROM public;

SELECT pg_reload_conf();
```

## 相关

- 智能体：`database-reviewer` — 完整数据库审查工作流
- 技能：`clickhouse-io` — ClickHouse 分析模式
- 技能：`backend-patterns` — API 和后端模式

---

*基于 Supabase Agent Skills（来源：Supabase 团队）（MIT License）*

---

---
name: database-migrations
description: 数据库迁移最佳实践，涵盖 schema 变更、数据迁移、回滚和跨 PostgreSQL、MySQL 及常见 ORM（Prisma、Drizzle、Kysely、Django、TypeORM、golang-migrate）的零停机部署。
origin: ECC
---

# 数据库迁移模式

生产系统的安全、可逆数据库 schema 变更。

## 激活时机

- 创建或修改数据库表
- 添加/删除列或索引
- 运行数据迁移（回填、转换）
- 规划零停机 schema 变更
- 为新项目设置迁移工具

## 核心原则

1. **每个变更都是迁移** — 绝不手动修改生产数据库
2. **生产中迁移仅向前** — 回滚使用新的前向迁移
3. **schema 和数据迁移分离** — 绝不混合 DDL 和 DML
4. **根据生产规模数据测试迁移** — 在 100 行上工作的迁移在 10M 行上可能锁定
5. **迁移一旦部署就不可变** — 绝不编辑已在生产中运行的迁移

## 迁移安全检查清单

应用任何迁移前：

- [ ] 迁移有 UP 和 DOWN（或明确标记为不可逆）
- [ ] 大表上无全表锁（使用并发操作）
- [ ] 新列有默认值或可空（绝不添加无默认值的 NOT NULL）
- [ ] 索引并发创建（现有表的 CREATE TABLE 内联不行）
- [ ] 数据回填与 schema 变更分离
- [ ] 根据生产数据副本测试
- [ ] 回滚计划已记录

## PostgreSQL 模式

### 安全添加列

```sql
-- 好：可空列，无锁
ALTER TABLE users ADD COLUMN avatar_url TEXT;

-- 好：有默认值的列（Postgres 11+ 是即时的，无需重写）
ALTER TABLE users ADD COLUMN is_active BOOLEAN NOT NULL DEFAULT true;

-- 坏：现有表上无默认值的 NOT NULL（需要完全重写）
ALTER TABLE users ADD COLUMN role TEXT NOT NULL;
-- 这会锁定表并重写每一行
```

### 无停机添加索引

```sql
-- 坏：大表上阻塞写入
CREATE INDEX idx_users_email ON users (email);

-- 好：非阻塞，允许并发写入
CREATE INDEX CONCURRENTLY idx_users_email ON users (email);

-- 注意：CONCURRENTLY 不能在事务块内运行
-- 大多数迁移工具需要特殊处理
```

### 重命名列（零停机）

绝不直接在生产中重命名。使用扩展-收缩模式：

```sql
-- 步骤 1：添加新列（迁移 001）
ALTER TABLE users ADD COLUMN display_name TEXT;

-- 步骤 2：回填数据（迁移 002，数据迁移）
UPDATE users SET display_name = username WHERE display_name IS NULL;

-- 步骤 3：更新应用程序代码读写两列
-- 部署应用程序变更

-- 步骤 4：停止写入旧列，删除它（迁移 003）
ALTER TABLE users DROP COLUMN username;
```

### 安全删除列

```sql
-- 步骤 1：移除所有对列的应用程序引用
-- 步骤 2：部署不带列引用的应用程序
-- 步骤 3：在下一个迁移中删除列
ALTER TABLE orders DROP COLUMN legacy_status;

-- 对于 Django：使用 SeparateDatabaseAndState 从模型中移除
-- 而不生成 DROP COLUMN（然后在下一个迁移中删除）
```

### 大数据迁移

```sql
-- 坏：单事务更新所有行（锁定表）
UPDATE users SET normalized_email = LOWER(email);

-- 好：批量更新带进度
DO $$
DECLARE
  batch_size INT := 10000;
  rows_updated INT;
BEGIN
  LOOP
    UPDATE users
    SET normalized_email = LOWER(email)
    WHERE id IN (
      SELECT id FROM users
      WHERE normalized_email IS NULL
      LIMIT batch_size
      FOR UPDATE SKIP LOCKED
    );
    GET DIAGNOSTICS rows_updated = ROW_COUNT;
    RAISE NOTICE 'Updated % rows', rows_updated;
    EXIT WHEN rows_updated = 0;
    COMMIT;
  END LOOP;
END $$;
```

## Prisma（TypeScript/Node.js）

### 工作流

```bash
# 从 schema 变更创建迁移
npx prisma migrate dev --name add_user_avatar

# 在生产中应用待处理迁移
npx prisma migrate deploy

# 重置数据库（仅开发）
npx prisma migrate reset

# schema 变更后生成客户端
npx prisma generate
```

### Schema 示例

```prisma
model User {
  id        String   @id @default(cuid())
  email     String   @unique
  name      String?
  avatarUrl String?  @map("avatar_url")
  createdAt DateTime @default(now()) @map("created_at")
  updatedAt DateTime @updatedAt @map("updated_at")
  orders    Order[]

  @@map("users")
  @@index([email])
}
```

### 自定义 SQL 迁移

对于 Prisma 无法表达的操作（并发索引、数据回填）：

```bash
# 创建空迁移，然后手动编辑 SQL
npx prisma migrate dev --create-only --name add_email_index
```

```sql
-- migrations/20240115_add_email_index/migration.sql
-- Prisma 无法生成 CONCURRENTLY，所以我们手动编写
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_users_email ON users (email);
```

## Drizzle（TypeScript/Node.js）

### 工作流

```bash
# 从 schema 变更生成迁移
npx drizzle-kit generate

# 应用迁移
npx drizzle-kit migrate

# 直接推送 schema（仅开发，无迁移文件）
npx drizzle-kit push
```

### Schema 示例

```typescript
import { pgTable, text, timestamp, uuid, boolean } from "drizzle-orm/pg-core";

export const users = pgTable("users", {
  id: uuid("id").primaryKey().defaultRandom(),
  email: text("email").notNull().unique(),
  name: text("name"),
  isActive: boolean("is_active").notNull().default(true),
  createdAt: timestamp("created_at").notNull().defaultNow(),
  updatedAt: timestamp("updated_at").notNull().defaultNow(),
});
```

## Kysely（TypeScript/Node.js）

### 工作流（kysely-ctl）

```bash
# 初始化配置文件（kysely.config.ts）
kysely init

# 创建新迁移文件
kysely migrate make add_user_avatar

# 应用所有待处理迁移
kysely migrate latest

# 回滚最后迁移
kysely migrate down

# 显示迁移状态
kysely migrate list
```

### 迁移文件

```typescript
// migrations/2024_01_15_001_create_user_profile.ts
import { type Kysely, sql } from 'kysely'

// 重要：始终使用 Kysely<any>，而非你类型化的 DB 接口。
// 迁移是时间冻结的，绝不能依赖当前 schema 类型。
export async function up(db: Kysely<any>): Promise<void> {
  await db.schema
    .createTable('user_profile')
    .addColumn('id', 'serial', (col) => col.primaryKey())
    .addColumn('email', 'varchar(255)', (col) => col.notNull().unique())
    .addColumn('avatar_url', 'text')
    .addColumn('created_at', 'timestamp', (col) =>
      col.defaultTo(sql`now()`).notNull()
    )
    .execute()

  await db.schema
    .createIndex('idx_user_profile_avatar')
    .on('user_profile')
    .column('avatar_url')
    .execute()
}

export async function down(db: Kysely<any>): Promise<void> {
  await db.schema.dropTable('user_profile').execute()
}
```

### 程序化迁移器

```typescript
import { Migrator, FileMigrationProvider } from 'kysely'
import { promises as fs } from 'fs'
import * as path from 'path'
// 仅 ESM — CJS 可直接使用 __dirname
import { fileURLToPath } from 'url'
const migrationFolder = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  './migrations',
)

// `db` 是你的 Kysely<any> 数据库实例
const migrator = new Migrator({
  db,
  provider: new FileMigrationProvider({
    fs,
    path,
    migrationFolder,
  }),
  // 警告：仅在开发中启用。禁用时间戳排序验证，
  // 这可能导致环境之间的 schema 漂移。
  // allowUnorderedMigrations: true,
})

const { error, results } = await migrator.migrateToLatest()

results?.forEach((it) => {
  if (it.status === 'Success') {
    console.log(`migration "${it.migrationName}" executed successfully`)
  } else if (it.status === 'Error') {
    console.error(`failed to execute migration "${it.migrationName}"`)
  }
})

if (error) {
  console.error('migration failed', error)
  process.exit(1)
}
```

## Django（Python）

### 工作流

```bash
# 从模型变更生成迁移
python manage.py makemigrations

# 应用迁移
python manage.py migrate

# 显示迁移状态
python manage.py showmigrations

# 为自定义 SQL 生成空迁移
python manage.py makemigrations --empty app_name -n description
```

### 数据迁移

```python
from django.db import migrations

def backfill_display_names(apps, schema_editor):
    User = apps.get_model("accounts", "User")
    batch_size = 5000
    users = User.objects.filter(display_name="")
    while users.exists():
        batch = list(users[:batch_size])
        for user in batch:
            user.display_name = user.username
        User.objects.bulk_update(batch, ["display_name"], batch_size=batch_size)

def reverse_backfill(apps, schema_editor):
    pass  # 数据迁移，不需要反向

class Migration(migrations.Migration):
    dependencies = [("accounts", "0015_add_display_name")]

    operations = [
        migrations.RunPython(backfill_display_names, reverse_backfill),
    ]
```

### SeparateDatabaseAndState

从 Django 模型中移除列而不立即从数据库删除：

```python
# 迁移 001: 应用程序部署，使用新列名
# 迁移 002: 数据回填完成后
# 迁移 003: 使用 SeparateDatabaseAndState 标记列为从模型移除
# 迁移 004: 几个月后，确认旧列不再被访问，删除它

from django.db import migrations

class Migration(migrations.Migration):
    operations = [
        migrations.SeparateDatabaseAndState(
            state_operations=[
                migrations.RemoveField(model_name='order', name='legacy_status'),
            ],
            database_operations=[
                # 数据库中实际不删除列 — 安全，因为应用程序不再使用它
            ],
        )
    ]
```

## 回滚策略

### 安全回滚模式

1. **新列回滚**：更新应用程序使用旧列，部署，然后删除新列
2. **索引回滚**：`DROP INDEX CONCURRENTLY IF EXISTS`
3. **表重命名回滚**：重命名新表回旧名（如果旧表存在）
4. **数据回滚**：编写前向迁移以恢复数据，从不需要回滚

### 永远不回滚的原因

- 数据已写入新列
- 其他服务可能依赖新 schema
- 外键约束使删除新列复杂

## 性能考虑

| 操作 | 大表风险 | 缓解 |
|------|---------|------|
| 添加 NOT NULL 列 | 全表重写 + 锁 | 使用默认值（Postgres 11+） |
| 添加有默认值列 | 全表重写 | Postgres 11+ 无重写 |
| 删除列 | 长时间锁 | 分阶段，先不写再删除 |
| 添加索引 | 表锁（除非 CONCURRENTLY） | 始终 CONCURRENTLY |
| 重命名列 | 全表锁 | 使用 expand-contract 模式 |
| 大数据更新 | 事务大小 + 锁 | 批量处理 + COMMIT |

## 相关技能

- `postgres-patterns` — PostgreSQL 模式快速参考
- `database-reviewer` — 完整数据库审查智能体
- `backend-patterns` — API 和后端模式