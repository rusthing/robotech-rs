# RoboTech-RS

[中文](README_zh.md)

RoboTech-RS is a backend service toolkit written in Rust. This project provides commonly used tools for RESTful
controller layer and business logic layer.

## Project Structure

This project organizes functional modules through Rust's feature flags mechanism:

- `api` feature (enabled by default): Contains API interfaces and data transfer objects, providing a unified response
  object (RO) structure
- `base` feature: Provides basic functionalities including logging, configuration management, environment variables
- `web` feature: Implements web server functionality based on Actix-web
- `crud` feature: Implements database operations based on SeaORM

## Tech Stack

- Rust 2024 edition
- Actix-web as Web framework (in web feature)
- SeaORM as database ORM (in crud feature)
- Utoipa for OpenAPI documentation generation (in api feature)
- Serde for serialization/deserialization (in api feature)
- Chrono for time processing (in api feature)
- Tracing ecosystem for logging (in base feature)

## Feature Flags

This project uses feature flags to control dependencies and functionality:

- `api` (default): Enables API-related features, including response objects (RO) and data transfer
- `base`: Enables basic features like logging, configuration, environment management
- `web`: Enables web server functionality (includes base feature)
- `crud`: Enables database CRUD operations (includes base feature)

The `api` feature is enabled by default. To use web server features, you can enable the web feature:

```toml
[dependencies.robotech]
version = "0.8.0"
features = ["web"]
```

To use database operations, enable the crud feature:

```toml
[dependencies.robotech]
version = "0.8.0"
features = ["web", "crud"]
```

## API Response Format

This project adopts a unified response format, where all API responses follow this structure:

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

API Response Fields Explanation:

- `result`: Response result (Success, IllegalArgument, Warn, Fail)
- `msg`: Response message
- `timestamp`: Timestamp
- `extra`: Optional extra data
- `detail`: Optional detailed information
- `code`: Optional business code

## Quick Start

### Prerequisites

- Rust 1.70 or higher
- PostgreSQL database (if using SeaORM)

### Build Project

```bash
# Build with default features (api_client)
cargo build

# Build with web server features
cargo build --features web

# Build with all features
cargo build --features web,crud
```

### Run Service

```bash
# Run service (requires web feature)
cargo run --features web

# Run service with database support
cargo run --features web,crud
```

## Modules

- `ro`: Response objects for API responses with unified format
- `cst`: Constants used across the application
- `ctrl`: Controllers handling HTTP requests
- `svc`: Business logic services
- `config`: Configuration management
- `web_server`: Web server implementation
- `db`: Database operations
- `log`: Logging functionality
- `env`: Environment variable management

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.