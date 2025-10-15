# RoboTech-rs

RoboTech-rs 是一个用 Rust 编写的后端工具库。该项目采用模块化设计，分为 API 定义和服务实现两个主要部分。

## 项目结构

本项目采用 Rust 工作区(workspace)的形式组织，包含以下两个主要 crate：

- [`robotech-api`](./robotech-api): 定义 API 接口和数据传输对象，提供统一的响应对象（RO）结构
- [`robotech-svr`](./robotech-svr): 实现服务端业务逻辑，包括控制器（ctrl）、服务（svc）和常量（cst）模块

## 技术栈

- Rust 2024 edition
- Actix-web 作为 Web 框架
- SeaORM 作为数据库 ORM
- Utoipa 用于 OpenAPI 文档生成
- Serde 用于序列化/反序列化
- Chrono 用于时间处理

## 快速开始

### 先决条件

- Rust 1.70 或更高版本
- PostgreSQL 数据库（如果使用 SeaORM）
- 环境变量配置（如数据库连接信息等）

### 构建项目

```bash
cargo build
```

### 运行服务

```bash
# 运行默认二进制文件
cargo run

# 或者运行特定的二进制文件（如果项目中有多个）
cargo run --bin <binary_name>
```

## API 响应格式

本项目采用统一的响应格式，所有 API 响应都遵循以下结构：

- `result`: 响应结果（Success, IllegalArgument, Warn, Fail）
- `msg`: 响应消息
- `timestamp`: 时间戳
- `extra`: 可选的额外数据
- `detail`: 可选的详细信息
- `code`: 可选的业务编码

## 许可证

本项目根据 MIT 许可证授权 - 查看 [LICENSE](LICENSE) 文件了解更多详情。