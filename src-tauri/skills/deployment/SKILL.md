---
name: deployment-patterns
description: 部署工作流、CI/CD 管道模式、Docker 容器化、健康检查、回滚策略和 Web 应用程序生产就绪检查清单。
origin: ECC
---

# 部署模式

生产部署工作流和 CI/CD 最佳实践。

## 激活时机

- 设置 CI/CD 管道
- 将应用程序容器化
- 规划部署策略（蓝绿、金丝雀、滚动）
- 实现健康检查和就绪探针
- 准备生产发布
- 配置环境特定设置

## 部署策略

### 滚动部署（默认）

逐步替换实例 — 推出期间旧版本和新版本同时运行。

```
Instance 1: v1 → v2  (先更新)
Instance 2: v1        (仍运行 v1)
Instance 3: v1        (仍运行 v1)

Instance 1: v2
Instance 2: v1 → v2  (第二个更新)
Instance 3: v1

Instance 1: v2
Instance 2: v2
Instance 3: v1 → v2  (最后更新)
```

**优点：** 零停机、逐步推出
**缺点：** 两个版本同时运行 — 需要向后兼容变更
**适用场景：** 标准部署、向后兼容变更

### 蓝绿部署

运行两个相同环境。原子性地切换流量。

```
Blue  (v1) ← 流量
Green (v2)   空闲，运行新版本

# 验证后：
Blue  (v1)   空闲（成为备用）
Green (v2) ← 流量
```

**优点：** 即时回滚（切换回 blue）、干净交接
**缺点：** 部署期间需要 2x 基础设施
**适用场景：** 关键服务、零问题容忍

### 金丝雀部署

先将一小部分流量路由到新版本。

```
v1: 95% 流量
v2:  5% 流量  (金丝雀)

# 如果指标看起来不错：
v1: 50% 流量
v2: 50% 流量

# 最终：
v2: 100% 流量
```

**优点：** 在全面推出前用真实流量捕获问题
**缺点：** 需要流量分割基础设施、监控
**适用场景：** 高流量服务、风险变更、功能标志

## 健康检查模式

### 应用层健康检查

```python
# 应用程序中的健康检查端点
# Flask
@app.route('/health')
def health():
    return {'status': 'healthy'}, 200

# FastAPI
@app.get("/health")
async def health():
    return {"status": "healthy"}
```

### 依赖健康检查

```python
# 带有依赖检查的健康检查
@app.route('/health')
def health():
    checks = {
        'database': check_db(),
        'cache': check_redis(),
        'external_api': check_external(),
    }
    all_healthy = all(checks.values())
    status_code = 200 if all_healthy else 503
    return {'checks': checks}, status_code
```

### Docker HEALTHCHECK

```dockerfile
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://localhost:3000/health || exit 1
```

### Kubernetes 就绪探针

```yaml
readinessProbe:
  httpGet:
    path: /health
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 10
  failureThreshold: 3

livenessProbe:
  httpGet:
    path: /health
    port: 3000
  initialDelaySeconds: 15
  periodSeconds: 20
  failureThreshold: 3
```

## 回滚策略

### 自动回滚触发器

```yaml
# Kubernetes 回滚
kubectl rollout undo deployment/myapp

# Docker Compose 回滚
docker compose down && docker compose -f docker-compose.prev.yml up -d

# Terraform 回滚
terraform apply -var-file=previous.tfvars
```

### 基于指标的自动回滚

```yaml
# 监控驱动的回滚策略
rollback:
  triggers:
    - metric: error_rate
      threshold: 5%
      window: 5m
    - metric: latency_p99
      threshold: 500ms
      window: 5m
    - metric: success_rate
      threshold: 95%
      window: 5m
```

### 回滚检查清单

- [ ] 部署前记录当前版本
- [ ] 保留前 3 个版本的容器镜像
- [ ] 数据库迁移设计为可逆
- [ ] 监控错误率和延迟
- [ ] 有明确的回滚阈值标准

## CI/CD 模式

### GitHub Actions 管道

```yaml
name: CI/CD

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - run: npm ci
      - run: npm test
      - run: npm run lint

  build:
    needs: test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: docker build -t app:${{ github.sha }} .
      - run: docker push registry/app:${{ github.sha }}

  deploy:
    needs: build
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    environment: production
    steps:
      - run: echo "Deploying ${{ github.sha }} to production"
```

### 多环境部署

```yaml
deploy:
  jobs:
    - name: deploy-staging
      environment: staging
      trigger: main
      verify:
        - smoke_tests
        - integration_tests

    - name: deploy-production
      environment: production
      trigger: manual
      requires:
        - deploy-staging
      verify:
        - canary_check
        - manual_approval
```

## 生产就绪检查清单

### 部署前

- [ ] 所有敏感配置在环境变量中
- [ ] 健康检查端点已实现
- [ ] 日志格式化为结构化 JSON
- [ ] 跟踪已配置（OpenTelemetry/DataDog）
- [ ] 指标已暴露（Prometheus 端点）

### 基础设施

- [ ] 数据库连接池配置正确
- [ ] 缓存层已设置（Redis）
- [ ] CDN 用于静态资产
- [ ] 负载均衡器配置
- [ ] 自动扩展策略定义

### 安全

- [ ] 容器以非 root 运行
- [ ] Secrets 通过 Vault 或环境注入
- [ ] TLS 终止在负载均衡器
- [ ] 防火墙规则最小化
- [ ] 定期安全扫描已配置

### 监控和告警

- [ ] 错误率仪表板
- [ ] 延迟 P50/P95/P99
- [ ] 容量使用（CPU/内存/磁盘）
- [ ] 业务指标（订单量、活跃用户）
- [ ] 告警阈值已设置
- [ ] 值班轮次已定义

### 容灾

- [ ] 数据库备份策略
- [ ] 多区域部署（如果需要高可用）
- [ ] 故障切换文档
- [ ] 联系信息和升级链

## 零停机部署技术

### 数据库迁移兼容性

```sql
-- 阶段 1: 添加新列（向后兼容）
ALTER TABLE users ADD COLUMN display_name VARCHAR(255);

-- 阶段 2: 更新应用程序读写两列
-- 部署新代码

-- 阶段 3: 停止写入旧列
-- 部署使用新列的代码

-- 阶段 4: 删除旧列（安全）
ALTER TABLE users DROP COLUMN name;
```

### 特性标志

```python
# 功能标志驱动部署
@app.route('/api/v2/data')
def api_v2():
    if not features.is_enabled('api_v2'):
        return api_v1()
    return new_implementation()
```

### 背靠背部署

```
步骤 1: 部署 v2（100% 流量仍路由到 v1）
步骤 2: 监控 v2 健康状况
步骤 3: 将 10% 流量切换到 v2
步骤 4: 监控错误率和延迟
步骤 5: 逐步增加直到 100%
步骤 6: v1 可以退役
```

## 性能考虑

| 因素 | 建议 |
|------|------|
| 镜像大小 | 使用多阶段构建，移除开发依赖 |
| 冷启动 | 保持镜像轻量，预热缓存 |
| 资源限制 | 设置 CPU/内存限制以防止资源耗尽 |
| 连接池 | 配置适合工作负载的池大小 |
| 超时 | 为外部服务设置合理的超时 |