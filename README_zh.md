# RoboTech-RS

[English](README.md)

RoboTech-RS 是一个用 Rust 编写的后端服务工具库。本项目提供 RESTful 控制器层和业务逻辑层的常用的工具。

## 项目结构

本项目通过 Rust 的 feature flags 机制组织功能模块：

- `api` feature（默认启用）：包含 API 接口和数据传输对象，提供统一的响应对象（RO）结构
- `svr` feature：实现服务端业务逻辑，包括控制器（ctrl）、服务（svc）和常量（cst）模块

## 技术栈

- Rust 2024 edition
- Actix-web 作为 Web 框架（在 svr feature 中）
- SeaORM 作为数据库 ORM（在 svr feature 中）
- Utoipa 用于 OpenAPI 文档生成（在 api feature 中）
- Serde 用于序列化/反序列化（在 api feature 中）
- Chrono 用于时间处理（在 api feature 中）

## Feature Flags

本项目使用 feature flags 来控制依赖和功能：

- `api`（默认）：启用 API 相关功能，包括响应对象（RO）和数据传输
- `svr`：启用服务端功能，包括控制器、服务和数据库操作

默认情况下启用 `api` feature。要使用服务端功能，可以同时启用两个 features：

```toml
[dependencies.robotech]
version = "0.3.2"
features = ["api", "svr"]
```

## API 响应格式

本项目采用统一的响应格式，所有 API 响应都遵循以下结构：

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

字段说明：
- `result`: 响应结果（Success, IllegalArgument, Warn, Fail）
- `msg`: 响应消息
- `timestamp`: 时间戳（毫秒）
- `extra`: 可选的额外数据
- `detail`: 可选的详细信息
- `code`: 可选的业务编码

## 快速开始

### 先决条件

- Rust 1.70 或更高版本
- PostgreSQL 数据库（如果使用 SeaORM）

### 构建项目

```bash
# 构建默认 features（api）
cargo build

# 构建所有 features
cargo build --features api,svr
```

### 运行服务

```bash
# 运行服务（需要启用 svr feature）
cargo run --features svr
```

## 模块说明

- `ro`: API响应对象，提供统一格式的响应
- `cst`: 应用中使用的常量
- `ctrl`: 处理HTTP请求的控制器
- `svc`: 业务逻辑服务
- `settings`: 配置管理
- `web_server`: Web服务器实现

## 许可证

本项目根据 MIT 许可证授权 - 查看 [LICENSE](LICENSE) 文件了解更多详情。