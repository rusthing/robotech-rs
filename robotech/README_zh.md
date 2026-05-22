# RoboTech-RS

[English](README.md)

RoboTech-RS 是一个用 Rust 编写的后端服务微服务框架。本项目提供 RESTful API 开发的核心组件，包括控制器层、业务逻辑层、数据访问层以及丰富的基础设施工具，极大简化 Rust 后端服务的开发复杂度。

## 项目结构

```
src/
├── api_client/         # API 客户端（可选 feature：api-client）
│                       # 提供 HTTP 客户端封装，用于调用其他微服务
├── app/               # 应用配置管理（feature：app）
│                       # 配置文件加载、应用上下文管理
├── cfg/               # 配置模块（feature：app）
│                       # 支持 TOML/YAML/JSON 格式配置文件
├── cst/               # 常量定义
│                       # 全局常量和枚举定义
├── dao/               # 数据访问层（feature：db）
│   ├── eo/            # Entity Object - 数据库实体映射
│                       # 基于 SeaORM Entity 封装
│   ├── dao_error.rs   # DAO 错误处理
│   └── dao_utils.rs   # DAO 工具函数
│                       # 通用 CRUD 操作封装
├── db/                # 数据库连接与迁移（feature：db）
│                       # 数据库连接池管理、迁移脚本执行
├── env/               # 环境变量管理（feature：app）
│                       # 环境变量读取和验证
├── log/               # 日志系统（feature：app）
│                       # 结构化日志，支持文件轮转、多输出源
├── macros/            # 宏定义（feature：macros）
│                       # 简化开发的常用宏
├── ro/                # 响应对象（Response Object）
│                       # 统一的 API 响应格式
├── signal/            # 信号处理（feature：app）
│                       # SIGTERM/SIGKILL 优雅停机支持
├── svc/               # 业务逻辑层（feature：app）
│                       # 通用业务逻辑基类和服务模板
└── web/               # Web 服务器（feature：web）
    ├── cors/          # CORS 跨域中间件
    ├── ctrl/          # 控制器基类
    │                       # BaseController 提供通用 HTTP 处理方法
    ├── health_check/  # 健康检查端点
    ├── https/         # HTTPS/TLS 支持
    ├── middleware/    # 中间件
    │                       # 请求日志、认证、限流等
    └── server/        # Web 服务器核心
            # Axum 服务器封装、多端口监听
```

### Feature Flags 机制

本项目通过 Rust 的 feature flags 机制组织功能模块，实现按需加载、减少依赖：

- **`app`**：应用配置管理，包含日志记录、配置管理、环境变量处理、信号处理
- **`web`**：基于 Axum 实现 Web 服务器功能（依赖 `app`），支持 HTTPS、CORS、中间件
- **`db`**：基于 SeaORM 实现数据库操作功能（依赖 `app`），提供 DAO 模式封装
- **`macros`**：宏定义，简化开发和代码生成
- **`api-client`**：HTTP 客户端封装，用于微服务间通信

注意：`web` 和 `db` 都依赖 `app` feature，即启用它们时会自动包含 `app` 功能。

## 技术栈

### 核心框架
- **Rust 2024 Edition**：现代系统编程语言，保证内存安全和零成本抽象
- **Axum**：Tokio 生态的 Ergonomic Web 框架，高性能且易用上
- **Tokio**：异步运行时，提供高并发处理能力
- **SeaORM**：异步 ORM 框架，类型安全、编译期 SQL 验证

### 配置与序列化
- **Serde**：序列化/反序列化框架
- **Config**：多层配置管理（文件、环境变量、命令行参数）
- **Validator**：数据验证框架
- **Chrono**：日期时间处理
- **Derive Setters / Typed Builder**：Builder 模式构建器

### 日志与监控
- **Tracing Ecosystem**：结构化日志，支持 JSON 格式
  - `tracing-subscriber`：日志订阅器和过滤器
  - `tracing-appender`：文件输出、日志轮转
  - `tracing-log`：兼容 `log` crate
- **Notify**：配置文件热重载监听

### API 文档
- **Utoipa**：OpenAPI 3.0 文档生成
- **Utoipa-Swagger-UI**：交互式 API 文档界面

### 网络与安全
- **Tower / Tower-HTTP**：中间件集合（CORS、追踪、静态文件）
- **Tokio-Rustls + AWS-LC-RS**：TLS 加密传输
- **IPNet**：IP 地址和网段处理
- **Reqwest**：HTTP 客户端

### 数据库
- **SQLx**：编译期 SQL 验证
- **SeaORM**：异步 ORM，支持 MySQL 和 PostgreSQL
- **Regex**：正则表达式匹配
- **Once Cell**：懒初始化全局状态

### 工具库
- **Anyhow**：错误处理框架
- **Thiserror**：自定义错误类型
- **Linkme**：堆内内存分配器
- **Nix**：Unix API 封装
- **Socket2**： Socket API 封装
- **IdWorker**：分布式唯一 ID 生成
- **Wheel-RS**：内部通用工具库

### 内部依赖
- **robotech-macros**：内部宏库，提供简化开发的辅助宏

## Feature Flags

本项目使用 feature flags 来控制依赖和功能模块，实现按需加载、减少二进制体积：

### 核心 Features

| Feature | 说明 | 依赖 | 默认 |
|---------|------|------|------|
| `app` | 应用配置、日志、环境变量、信号处理 | - | ❌ |
| `web` | Web 服务器（包含 `app`） | `app`, `axum`, `tower` | ❌ |
| `db` | 数据库操作（包含 `app`） | `app`, `sea-orm`, `sqlx` | ❌ |
| `macros` | 宏定义 | - | ❌ |
| `api-client` | HTTP 客户端 | `reqwest` | ❌ |

### 推荐组合

**仅 API 服务**（最小组合）：
```toml
[dependencies.robotech]
version = "1.5.1"
features = ["app"]
```

**完整 Web 服务 + 数据库**：
```toml
[dependencies.robotech]
version = "1.5.1"
features = ["web", "db", "macros"]
```

**微服务场景（含客户端）**：
```toml
[dependencies.robotech]
version = "1.5.1"
features = ["web", "db", "macros", "api-client"]
```

## 模块说明

### 核心模块

- **`ro`**: API响应对象（Response Object），提供统一格式的响应结构
  - `RO<T>`：泛型响应包装
  - `Result`：结果类型别名
  - `rx`：响应扩展方法

- **`cst`**: 常量定义
  - 全局常量和枚举
  - 业务状态码定义

### 应用模块（feature: app）

- **`cfg`**: 配置管理
  - 多层配置加载（文件、环境变量、命令行）
  - 配置文件热重载监听
  - 配置验证

- **`log`**: 日志系统
  - Tracing 框架集成
  - 结构化 JSON 日志输出
  - 文件轮转和动态级别调整

- **`env`**: 环境变量管理
  - 环境变量读取和验证
  - 默认值处理

- **`signal`**: 信号处理
  - Unix 信号监听（SIGTERM, SIGINT）
  - 优雅停机支持
  - PID 文件管理

- **`svc`**: 业务逻辑层基类
  - BaseSvc 通用服务模板
  - 事务管理辅助

- **`app`**: 应用上下文
  - 全局配置存储
  - 应用生命周期管理

### Web 模块（feature: web）

- **`web::server`**: Web 服务器核心
  - Axum 服务器封装
  - 多端口监听
  - HTTPS/TLS 支持

- **`web::ctrl`**: 控制器基类
  - BaseController trait
  - 自动错误处理和响应包装

- **`web::cors`**: CORS 中间件
  - 自定义跨域规则
  - 预检请求处理

- **`web::middleware`**: 中间件集合
  - TraceLayer：请求追踪和日志
  - IpFilter：IP 白名单/黑名单
  - HealthCheck：健康检查端点

- **`web::health_check`**: 健康检查
  - `/health` 端点实现
  - 服务状态报告

- **`web::https`**: HTTPS 支持
  - TLS 证书加载
  - HTTP/2 协议支持

### 数据库模块（feature: db）

- **`dao`**: 数据访问层
  - `BaseDAO` trait：泛型 CRUD 操作
  - `EO`（Entity Object）：数据库实体映射
  - DAO 工具函数
  - 外键和唯一键处理

- **`db`**: 数据库连接
  - 连接池管理
  - 迁移脚本执行
  - SQL 日志记录

### 宏系统（feature: macros）

- **`macros`**: 简化开发的宏
  - `build_app_cfg!`：构建配置
  - `watch_cfg_file!`：监听配置变化
  - `db_migrate!`：执行迁移
  - `log_call`：自动日志记录
  - 更多见 [robotech-macros](../robotech-macros/)

### API 客户端（feature: api-client）

- **`api_client`**: HTTP 客户端封装
  - 微服务间调用
  - Multipart 上传支持
  - 流式响应处理

## 核心特性

### 1. 统一的 API 响应格式

所有 API 响应遵循统一结构，前端可以轻松解析：

```json
{
  "result": 1,
  "msg": "Operation completed successfully",
  "timestamp": 1700000000000,
  "extra": {},
  "detail": "Optional detailed information",
  "code": "Optional business code"
}
```

**字段说明**：
- `result`：响应结果码（Success=1, Fail=-1 等）
- `msg`：响应消息提示
- `timestamp`：时间戳（毫秒）
- `extra`：扩展数据字段
- `detail`：详细信息（列表、分页等）
- `code`：业务编码（用于国际化）

**使用示例**：
```rust
use robotech::ro::{RO, Result};

// 成功响应
RO::success().finish()

// 带详情响应
RO::success().detail(vec![item1, item2]).finish()

// 错误响应
RO::illegal_argument("Invalid input").finish()
```

### 2. BaseDAO 数据访问层

提供通用的 CRUD 操作，大幅减少重复代码：

**核心功能**：
- `create`：创建记录
- `update`：更新记录
- `delete_by_id`：根据 ID 删除
- `get_by_id`：根据 ID 查询
- `list`：列出所有记录
- `page_list`：分页查询
- `exists`：判断记录是否存在

**使用示例**：
```rust
use robotech::dao::BaseDAO;
use sea_orm::EntityTrait;

#[derive(EntityTrait)]
struct MyEntity;

// 自动生成 CRUD 方法
impl_base_dao!(MyEntity);

// 使用
let entity = MyEntity::create(&ctx, data).await?;
let list = MyEntity::list(&ctx).await?;
let page = MyEntity::page_list(&ctx, page_num, page_size).await?;
```

### 3. BaseController 控制器层

提供 HTTP 请求处理基类，简化路由控制：

**核心功能**：
- 统一异常处理和错误转换
- 自动包装响应为 RO 格式
- 内置中间件支持
- CORS、认证、限流等常用中间件开箱即用

**使用示例**：
```rust
use robotech::web::ctrl::BaseController;

struct MyController;

#[async_trait]
impl BaseController for MyController {
    // 定义路由和处理方法
    async fn get_list(&self, state: AppState) -> impl IntoResponse {
        self.success_response(state).await
    }
}
```

### 4. 配置管理系统

支持多层配置优先级：命令行参数 > 环境变量 > 配置文件 > 默认值

**配置加载流程**：
1. 启动时加载配置文件（TOML/YAML/JSON）
2. 环境变量覆盖配置项
3. 命令行参数最高优先级
4. 配置文件变化时自动热重载

**使用示例**：
```rust
use robotech::cfg::build_app_cfg;

// 加载配置
let config = build_app_cfg::<AppConfig>("config.toml")?;

// 监听配置变化
watch_cfg_file!("app", {
    let new_config = build_app_cfg::<AppConfig>("config.toml")?;
    apply_new_config(new_config);
});
```

### 5. 日志系统

基于 Tracing 框架的结构化日志，生产环境开箱即用：

**核心功能**：
- 多输出源（控制台、文件）
- 日志轮转（按大小、按时间）
- JSON 格式输出（便于 ELK/Loki 收集）
- 动态日志级别调整
- 调用链追踪（span/span树）

**使用示例**：
```rust
use robotech::log::init_log;

// 初始化日志
init_log()?;

// 基本日志
tracing::info!("Service started");
tracing::debug!(user_id = 123, "Processing user");
tracing::warn!("High memory usage: {}MB", mem_usage);
tracing::error!(error = %e, "Failed to process request");

// Span 追踪
tracing::instrument(name = "process_request", fields(user_id))
async fn handle_request(id: u64) {
    tracing::debug!("Handling request");
    // ... 业务逻辑
}
```

### 6. 数据库连接与迁移

自动管理数据库连接池和执行迁移：

**核心功能**：
- 连接池管理（最大/最小连接数）
- 自动迁移执行（启动时升级数据库 schema）
- SQL 日志记录
- 事务支持

**使用示例**：
```rust
use robotech::db::init_db_conn;

// 初始化数据库连接
init_db_conn(config.clone()).await?;

// 自动迁移
db_migrate!("postgres://localhost/mydb");

// 查询
let entities = MyEntity::find().all(db).await?;
```

### 7. 信号处理与优雅停机

支持 Unix 信号，实现优雅停机：

**支持的信号**：
- `SIGTERM`（15）：优雅停机
- `SIGINT`（2）：中断
- `SIGUSR1/USR2`：自定义处理

**信号指令**：
- `start`：默认值，先发送 SIGCONT 检查是否已运行
- `restart`：先 SIGTERM 停止旧进程，再启动新进程
- `stop/s`：发送 SIGTERM 优雅停机
- `kill/k`：发送 SIGKILL 强制终止

**使用示例**：
```rust
use robotech::signal::SignalManager;

let (mut mgr, old_pid) = SignalManager::new("start".to_string())?;
let receiver = mgr.watch_signal()?;

// 等待信号并优雅关闭
wait_app_exit(signal_receiver, || async move {
    stop_services().await.ok();
    Ok(())
}).await?
```

### 8. 中间件系统

内置多种常用中间件：

| 中间件 | 功能 | 说明 |
|--------|------|------|
| CORS | 跨域资源共享 | 支持自定义域名、方法、头 |
| Trace | 请求追踪 | 记录请求耗时、状态码 |
| IpFilter | IP 过滤 | 支持白名单/黑名单 |
| Health Check | 健康检查 | `/health` 端点 |
| Auth | 认证中间件 | JWT/OAuth2（需自定义） |

**添加中间件**：
```rust
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

let app = Router::new()
    .route("/api/list", get(handler))
    .layer(TraceLayer::new_for_axum())
    .layer(CorsLayer::permissive());
```

## 快速开始

### 先决条件

- Rust 1.75+（2024 Edition）
- PostgreSQL 12+ 或 MySQL 8.0+（如果使用数据库）
- Redis（可选，用于缓存）

### 第一步：创建项目

```bash
cargo new my-service
cd my-service
```

### 第二步：添加依赖

```toml
# Cargo.toml
[dependencies]
robotech = { version = "1.5.1", features = ["web", "db", "macros"] }
tokio = { version = "1", features = ["full"] }
```

### 第三步：编写代码

```rust
// main.rs
use robotech::app::AppConfig;
use robotech::db::init_db_conn;
use robotech::web::start_web_server;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct MyAppConfig {
    // 自定义配置
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 初始化
    robotech::env::init_env()?;
    robotech::log::init_log()?;
    
    // 2. 加载配置
    let (config, _) = robotech::cfg::build_app_cfg::<AppConfig>(None)?;
    
    // 3. 数据库连接
    init_db_conn(config.db.clone()).await?;
    
    // 4. 启动 Web 服务
    start_web_server(config.web_server, None, None).await?;
    
    Ok(())
}
```

### 第四步：配置数据库

创建数据库和用户：

```sql
-- PostgreSQL
CREATE USER myuser WITH PASSWORD 'mypassword';
CREATE DATABASE mydb OWNER myuser;

-- MySQL
CREATE USER 'myuser'@'%' IDENTIFIED BY 'mypassword';
CREATE DATABASE mydb CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
GRANT ALL PRIVILEGES ON mydb.* TO 'myuser'@'%';
```

### 第五步：运行服务

```bash
# 开发模式
cargo run --features web,db

# 指定配置文件
cargo run --features web,db -c ./config.toml

# 发布模式
cargo build --release
cargo run --features web,db -- -c ./config.toml
```

访问 Swagger UI：http://localhost:9840/swagger-ui/

## 快速开始

### 先决条件

- Rust 1.70 或更高版本
- PostgreSQL 数据库（如果使用 SeaORM）

### 构建项目

```bash
# 构建默认 features（api_client）
cargo build

# 构建 web 服务器功能
cargo build --features web

# 构建所有 features
cargo build --features web,db
```

### 运行服务

```bash
# 运行服务（需要启用 web feature）
cargo run --features web

# 运行带数据库支持的服务
cargo run --features web,db
```

## 核心概念

### RO (Response Object) - 响应对象

所有 API 响应都通过 `RO` 结构包装，确保统一的返回格式。

### BaseDAO - 数据访问基类

泛型封装了常见的 CRUD 操作，只需几行代码即可实现完整的数据库访问层。

### EO (Entity Object) - 实体对象

对应数据库表的 SeaORM Entity，由 `robotech-macros` 自动生成。

### DTO/VO - 数据传输/视图对象

- **DTO**（Data Transfer Object）：接收前端请求参数
- **VO**（View Object）：返回给前端的数据结构

### Service - 业务逻辑层

继承 `BaseSvc` 或自行实现业务逻辑，调用 DAO 完成数据操作。

### Controller - 控制器层

处理 HTTP 请求，调用 Service 层，返回 RO 响应。

## 宏系统（robotech-macros）

项目提供了丰富的宏来简化开发：

### 应用配置宏

- `build_app_cfg!` - 构建应用配置
- `watch_cfg_file!` - 监听配置文件变化并热重载

### 数据库宏

- `db_migrate!` - 执行数据库迁移
- `db_conn!` - 获取数据库连接

### 日志宏

- `log_call` - 自动记录函数调用日志
- `log_result` - 记录返回值

### 控制器宏

- `impl_controller` - 自动生成控制器路由

完整文档见：[robotech-macros](../robotech-macros/README.md)

## 许可证

本项目根据 MIT 许可证授权 - 查看 [LICENSE](LICENSE) 文件了解更多详情。

## 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建特性分支（`git checkout -b feature/AmazingFeature`）
3. 提交更改（`git commit -m 'Add some AmazingFeature'`）
4. 推送到分支（`git push origin feature/AmazingFeature`）
5. 开启 Pull Request

## 联系方式

- 项目主页：https://github.com/rusthing/robotech-rs
- 问题反馈：https://github.com/rusthing/robotech-rs/issues
- 作者：zbz