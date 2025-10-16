# RoboTech-RS

[中文](README_zh.md)

RoboTech-RS is a backend service toolkit written in Rust. This project provides commonly used tools for RESTful
controller layer and business logic layer.

## Project Structure

This project organizes functional modules through Rust's feature flags mechanism:

- `api` feature (enabled by default): Contains API interfaces and data transfer objects, providing a unified response
  object (RO) structure
- `svr` feature: Implements server-side business logic, including controller (ctrl), service (svc), and constant (cst)
  modules

## Tech Stack

- Rust 2024 edition
- Actix-web as Web framework (in svr feature)
- SeaORM as database ORM (in svr feature)
- Utoipa for OpenAPI documentation generation (in api feature)
- Serde for serialization/deserialization (in api feature)
- Chrono for time processing (in api feature)

## Feature Flags

This project uses feature flags to control dependencies and functionality:

- `api` (default): Enables API-related features, including response objects (RO) and data transfer
- `svr`: Enables server-side features, including controllers, services, and database operations

The `api` feature is enabled by default. To use server-side features, you can enable both features:

```toml
[dependencies.robotech]
version = "0.3.2"
features = ["api", "svr"]
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
# Build with default features (api)
cargo build

# Build with all features
cargo build --features api,svr
```

### Run Service

```bash
# Run service (requires svr feature)
cargo run --features svr
```

## Modules

- `ro`: Response objects for API responses with unified format
- `cst`: Constants used across the application
- `ctrl`: Controllers handling HTTP requests
- `svc`: Business logic services
- `settings`: Configuration management
- `web_server`: Web server implementation

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.