---
name: java-coding-standards
description: Spring Boot 服务的 Java 编码标准：命名、不可变性、Optional 使用、流、异常、泛型和项目布局。
origin: ECC
---

# Java 编码标准

Spring Boot 服务中可读、可维护 Java（17+）代码的标准。

## 激活时机

- 在 Spring Boot 项目中编写或审查 Java 代码
- 强制执行命名、不可变性或异常处理约定
- 使用记录、密封类或模式匹配（Java 17+）
- 审查 Optional、流或泛型的使用
- 构建包和项目布局

## 核心原则

- 优先清晰性而非巧妙性
- 默认不可变；最小化共享可变状态
- 快速失败并抛出有意义的异常
- 一致的命名和包结构

## 命名

```java
// 通过：类/记录：PascalCase
public class MarketService {}
public record Money(BigDecimal amount, Currency currency) {}

// 通过：方法/字段：camelCase
private final MarketRepository marketRepository;
public Market findBySlug(String slug) {}

// 通过：常量：UPPER_SNAKE_CASE
private static final int MAX_PAGE_SIZE = 100;
```

## 不可变性

```java
// 通过：优先使用记录和 final 字段
public record MarketDto(Long id, String name, MarketStatus status) {}

public class Market {
  private final Long id;
  private final String name;
  // 仅 getter，无 setter
}
```

## Optional 使用

```java
// 通过：从 find* 方法返回 Optional
Optional<Market> market = marketRepository.findBySlug(slug);

// 通过：使用 map/flatMap 而非 get()
return market
    .map(MarketResponse::from)
    .orElseThrow(() -> new EntityNotFoundException("Market not found"));
```

## 流最佳实践

```java
// 通过：使用流进行转换，保持管道短
List<String> names = markets.stream()
    .map(Market::name)
    .filter(Objects::nonNull)
    .toList();

// 失败：避免复杂的嵌套流；为清晰起见优先使用循环
```

## 异常

- 对领域错误使用非受检异常；用上下文包装技术异常
- 创建领域特定异常（如 `MarketNotFoundException`）
- 除非重新抛出/集中记录，否则避免广泛的 `catch (Exception ex)`

```java
throw new MarketNotFoundException(slug);
```

## 泛型和类型安全

- 避免原始类型；声明泛型参数
- 对可重用工具优先使用有界泛型

```java
public <T extends Identifiable> Map<Long, T> indexById(Collection<T> items) { ... }
```

## 项目结构（Maven/Gradle）

```
src/main/java/com/example/app/
  config/
  controller/
  service/
  repository/
  domain/
  dto/
  util/
src/main/resources/
  application.yml
src/test/java/... (镜像 main)
```

## 格式化和样式

- 一致使用 2 或 4 个空格（项目标准）
- 每个文件一个公共顶层类型
- 保持方法简短专注；提取辅助方法
- 成员顺序：常量、字段、构造函数、公共方法、protected、私有

## 应避免的代码味道

- 长参数列表 → 使用 DTO/构建器
- 深层嵌套 → 提前返回
- 魔法数字 → 命名常量
- 静态可变状态 → 优先依赖注入
- 静默 catch 块 → 记录并行动或重新抛出

## 日志

```java
private static final Logger log = LoggerFactory.getLogger(MarketService.class);
log.info("fetch_market slug={}", slug);
log.error("failed_fetch_market slug={}", slug, ex);
```

## 空处理

- 仅在不可避免时接受 `@Nullable`；否则使用 `@NonNull`
- 在输入上使用 Bean 验证（`@NotNull`、`@NotBlank`）

## 测试期望

- JUnit 5 + AssertJ 用于流畅断言
- Mockito 用于 mock；尽可能避免部分 mock
- 优先确定性测试；无隐藏 sleep

**记住**：保持代码有意图、有类型、可观察。优化可维护性而非微优化，除非已证明必要。

---

---
name: jpa-patterns
description: JPA/Hibernate 模式，用于 Spring Boot 中的实体设计、关系、查询优化、事务、审计、索引、分页和连接池。
origin: ECC
---

# JPA/Hibernate 模式

用于 Spring Boot 中的数据建模、仓库和性能调优。

## 激活时机

- 设计 JPA 实体和表映射
- 定义关系（@OneToMany、@ManyToOne、@ManyToMany）
- 优化查询（N+1 预防、获取策略、投影）
- 配置事务、审计或软删除
- 设置分页、排序或自定义仓库方法
- 调优连接池（HikariCP）或二级缓存

## 实体设计

```java
@Entity
@Table(name = "markets", indexes = {
  @Index(name = "idx_markets_slug", columnList = "slug", unique = true)
})
@EntityListeners(AuditingEntityListener.class)
public class MarketEntity {
  @Id @GeneratedValue(strategy = GenerationType.IDENTITY)
  private Long id;

  @Column(nullable = false, length = 200)
  private String name;

  @Column(nullable = false, unique = true, length = 120)
  private String slug;

  @Enumerated(EnumType.STRING)
  private MarketStatus status = MarketStatus.ACTIVE;

  @CreatedDate private Instant createdAt;
  @LastModifiedDate private Instant updatedAt;
}
```

启用审计：
```java
@Configuration
@EnableJpaAuditing
class JpaConfig {}
```

## 关系和 N+1 预防

```java
@OneToMany(mappedBy = "market", cascade = CascadeType.ALL, orphanRemoval = true)
private List<PositionEntity> positions = new ArrayList<>();
```

- 默认延迟加载；在需要时在查询中使用 `JOIN FETCH`
- 避免在集合上使用 `EAGER`；对读取路径使用 DTO 投影

```java
@Query("select m from MarketEntity m left join fetch m.positions where m.id = :id")
Optional<MarketEntity> findWithPositions(@Param("id") Long id);
```

## 仓库模式

```java
public interface MarketRepository extends JpaRepository<MarketEntity, Long> {
  Optional<MarketEntity> findBySlug(String slug);

  @Query("select m from MarketEntity m where m.status = :status")
  Page<MarketEntity> findByStatus(@Param("status") MarketStatus status, Pageable pageable);
}
```

- 对轻量级查询使用投影：
```java
public interface MarketSummary {
  Long getId();
  String getName();
  MarketStatus getStatus();
}
Page<MarketSummary> findAllBy(Pageable pageable);
```

## 事务

- 用 `@Transactional` 注解服务方法
- 对读取路径使用 `@Transactional(readOnly = true)` 进行优化
- 仔细选择传播；避免长运行事务

```java
@Transactional
public Market updateStatus(Long id, MarketStatus status) {
  MarketEntity entity = repo.findById(id)
      .orElseThrow(() -> new EntityNotFoundException("Market"));
  entity.setStatus(status);
  return Market.from(entity);
}
```

## 分页

```java
PageRequest page = PageRequest.of(pageNumber, pageSize, Sort.by("createdAt").descending());
Page<MarketEntity> markets = repo.findByStatus(MarketStatus.ACTIVE, page);
```

对于游标式分页，在 JPQL 中包含 `id > :lastId` 并排序。

## 索引和性能

- 为常见过滤器添加索引（`status`、`slug`、外键）
- 使用匹配查询模式的复合索引（`status, created_at`）
- 避免 `select *`；仅投影需要的列
- 使用 `saveAll` 和 `hibernate.jdbc.batch_size` 批量写入

## 连接池（HikariCP）

推荐属性：
```
spring.datasource.hikari.maximum-pool-size=20
spring.datasource.hikari.minimum-idle=5
spring.datasource.hikari.connection-timeout=30000
spring.datasource.hikari.validation-timeout=5000
```

对于 PostgreSQL LOB 处理，添加：
```
spring.jpa.properties.hibernate.jdbc.lob.non_contextual_creation=true
```

## 缓存

- 一级缓存是 per EntityManager；避免跨事务保留实体
- 对于读取密集型实体，谨慎考虑二级缓存；验证驱逐策略

## 迁移

- 使用 Flyway 或 Liquibase；绝不依赖生产中的 Hibernate auto DDL
- 保持迁移幂等且增量；无计划地不删除列

## 测试数据访问

- 优先使用 `@DataJpaTest` 和 Testcontainers 来镜像生产
- 使用日志断言 SQL 效率：设置 `logging.level.org.hibernate.SQL=DEBUG` 和 `logging.level.org.hibernate.orm.jdbc.bind=TRACE` 用于参数值

**记住**：保持实体精简、查询有意图、事务短。通过获取策略和投影防止 N+1，并为读写路径建立索引。