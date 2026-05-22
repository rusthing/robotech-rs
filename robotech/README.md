# RoboTech-RS

[中文](README_zh.md)

RoboTech-RS is a microservice framework written in Rust for backend service development. This project provides core components for RESTful API development, including controller layer, business logic layer, data access layer, and extensive infrastructure tools, significantly simplifying the complexity of developing Rust backend services.

## Project Structure

```
src/
├── api_client/         # API Client (optional feature: api-client)
│                       # HTTP client wrapper for microservice communication
├── app/               # Application configuration management (feature: app)
│                       # Config file loading, application context
├── cfg/               # Configuration module (feature: app)
│                       # Supports TOML/YAML/JSON config formats
├── cst/               # Constants definition
│                       # Global constants and enums
├── dao/               # Data Access Object (feature: db)
│   ├── eo/            # Entity Object - Database entity mapping
│                       # Based on SeaORM Entity encapsulation
│   ├── dao_error.rs   # DAO error handling
│   └── dao_utils.rs   # DAO utility functions
│                       # Generic CRUD operation wrappers
├── db/                # Database connection and migration (feature: db)
│                       # Connection pool management, migration execution
├── env/               # Environment variable management (feature: app)
│                       # Environment variable reading and validation
├── log/               # Logging system (feature: app)
│                       # Structured logging with file rotation
├── macros/            # Macro definitions (feature: macros)
│                       # Development helper macros
├── ro/                # Response Object
│                       # Unified API response format
├── signal/            # Signal handling (feature: app)
│                       # SIGTERM/SIGKILL graceful shutdown support
├── svc/               # Business logic layer (feature: app)
│                       # Base service logic templates
└── web/               # Web server (feature: web)
    ├── cors/          # CORS middleware
    ├── ctrl/          # Controller base class
    │                       # BaseController for generic HTTP handling
    ├── health_check/  # Health check endpoint
    ├── https/         # HTTPS/TLS support
    ├── middleware/    # Middleware collection
    │                       # Request logging, auth, rate limiting
    └── server/        # Web server core
            # Axum server wrapper, multi-port listening
```

### Feature Flags Mechanism

This project organizes functional modules through Rust's feature flags mechanism for on-demand loading and dependency optimization:

- **`app`**: Application configuration management, including logging, configuration, environment variables, signal handling
- **`web`**: Web server implementation based on Axum (depends on `app`), supports HTTPS, CORS, middleware
- **`db`**: Database operations based on SeaORM (depends on `app`), provides DAO pattern encapsulation
- **`macros`**: Macro definitions for development simplification
- **`api-client`**: HTTP client wrapper for inter-service communication

Note: Both `web` and `db` depend on `app` feature, meaning enabling them automatically includes `app` functionality.

## Tech Stack

### Core Framework
- **Rust 2024 Edition**: Modern systems programming language ensuring memory safety and zero-cost abstractions
- **Axum**: Ergonomic web framework in Tokio ecosystem, high performance and ergonomic
- **Tokio**: Async runtime providing high-concurrency processing capabilities
- **SeaORM**: Async ORM framework with type safety and compile-time SQL verification

### Configuration & Serialization
- **Serde**: Serialization/deserialization framework
- **Config**: Multi-layer configuration management (files, env vars, CLI args)
- **Validator**: Data validation framework
- **Chrono**: Date and time processing
- **Derive Setters / Typed Builder**: Builder pattern constructors

### Logging & Monitoring
- **Tracing Ecosystem**: Structured logging with JSON output
  - `tracing-subscriber`: Log subscriber and filters
  - `tracing-appender`: File output and log rotation
  - `tracing-log`: Compatibility with `log` crate
- **Notify**: File system events for config hot-reload

### API Documentation
- **Utoipa**: OpenAPI 3.0 documentation generation
- **Utoipa-Swagger-UI**: Interactive API documentation UI

### Networking & Security
- **Tower / Tower-HTTP**: Middleware collection (CORS, tracing, static files)
- **Tokio-Rustls + AWS-LC-RS**: TLS encryption support
- **IPNet**: IP address and subnet handling
- **Reqwest**: HTTP client

### Database
- **SQLx**: Compile-time SQL verification
- **SeaORM**: Async ORM supporting MySQL and PostgreSQL
- **Regex**: Regular expression matching
- **Once Cell**: Lazy initialization global state

### Utilities
- **Anyhow**: Error handling framework
- **Thiserror**: Custom error types
- **Linkme**: In-memory allocator
- **Nix**: Unix API wrapper
- **Socket2**: Socket API wrapper
- **IdWorker**: Distributed unique ID generation
- **Wheel-RS**: Internal通用 utility library

### Internal Dependencies
- **robotech-macros**: Internal macro library for development acceleration

## Feature Flags

This project uses feature flags to control dependencies and functional modules for on-demand loading and reduced binary size:

### Core Features

| Feature | Description | Dependencies | Default |
|---------|-------------|--------------|----------|
| `app` | App config, logging, env vars, signals | - | ❌ |
| `web` | Web server (includes `app`) | `app`, `axum`, `tower` | ❌ |
| `db` | Database operations (includes `app`) | `app`, `sea-orm`, `sqlx` | ❌ |
| `macros` | Macro definitions | - | ❌ |
| `api-client` | HTTP client | `reqwest` | ❌ |

### Recommended Combinations

**Minimal API Service (smallest combination)**:
```toml
[dependencies.robotech]
version = "1.5.1"
features = ["app"]
```

**Full Web Service + Database**:
```toml
[dependencies.robotech]
version = "1.5.1"
features = ["web", "db", "macros"]
```

**Microservice Scenario (with client)**:
```toml
[dependencies.robotech]
version = "1.5.1"
features = ["web", "db", "macros", "api-client"]
```

## Core Features

### 1. Unified API Response Format

All API responses follow a unified structure for easy frontend parsing:

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

**Field Descriptions**:
- `result`: Response result code (Success=1, Fail=-1, etc.)
- `msg`: Response message prompt
- `timestamp`: Timestamp in milliseconds
- `extra`: Extended data field
- `detail`: Detailed information (lists, pagination, etc.)
- `code`: Business code (for internationalization)

**Usage Example**:
```rust
use robotech::ro::{RO, Result};

// Success response
RO::success().finish()

// Response with details
RO::success().detail(vec![item1, item2]).finish()

// Error response
RO::illegal_argument("Invalid input").finish()
```

### 2. BaseDAO Data Access Layer

Provides generic CRUD operations, drastically reducing repetitive code:

**Core Functions**:
- `create`: Create record
- `update`: Update record
- `delete_by_id`: Delete by ID
- `get_by_id`: Get by ID
- `list`: List all records
- `page_list`: Paginated query
- `exists`: Check if record exists

**Usage Example**:
```rust
use robotech::dao::BaseDAO;
use sea_orm::EntityTrait;

#[derive(EntityTrait)]
struct MyEntity;

// Auto-generate CRUD methods
impl_base_dao!(MyEntity);

// Usage
let entity = MyEntity::create(&ctx, data).await?;
let list = MyEntity::list(&ctx).await?;
let page = MyEntity::page_list(&ctx, page_num, page_size).await?;
```

### 3. BaseController Controller Layer

Provides HTTP request handling base class for simplified routing control:

**Core Functions**:
- Unified exception handling and error conversion
- Automatic response wrapping in RO format
- Built-in middleware support
- Common middleware like CORS, auth, rate limiting out-of-the-box

**Usage Example**:
```rust
use robotech::web::ctrl::BaseController;

struct MyController;

#[async_trait]
impl BaseController for MyController {
    async fn get_list(&self, state: AppState) -> impl IntoResponse {
        self.success_response(state).await
    }
}
```

### 4. Configuration Management System

Supports multi-layer configuration priority: CLI args > Environment variables > Config file > Defaults

**Configuration Loading Flow**:
1. Load config file on startup (TOML/YAML/JSON)
2. Override with environment variables
3. CLI args have highest priority
4. Auto hot-reload when config file changes

**Usage Example**:
```rust
use robotech::cfg::build_app_cfg;

// Load configuration
let config = build_app_cfg::<AppConfig>("config.toml")?;

// Watch for config changes
watch_cfg_file!("app", {
    let new_config = build_app_cfg::<AppConfig>("config.toml")?;
    apply_new_config(new_config);
});
```

### 5. Logging System

Structured logging based on Tracing framework, production-ready out-of-the-box:

**Core Functions**:
- Multiple output sources (console, file)
- Log rotation (by size, by time)
- JSON format output (for ELK/Loki collection)
- Dynamic log level adjustment
- Call chain tracing (span/span tree)

**Usage Example**:
```rust
use robotech::log::init_log;

// Initialize logging
init_log()?;

// Basic logging
tracing::info!("Service started");
tracing::debug!(user_id = 123, "Processing user");
tracing::warn!("High memory usage: {}MB", mem_usage);
tracing::error!(error = %e, "Failed to process request");

// Span tracing
tracing::instrument(name = "process_request", fields(user_id))
async fn handle_request(id: u64) {
    tracing::debug!("Handling request");
    // ... business logic
}
```

### 6. Database Connection & Migration

Automatically manages database connection pool and executes migrations:

**Core Functions**:
- Connection pool management (max/min connections)
- Automatic migration execution (upgrade DB schema on startup)
- SQL query logging
- Transaction support

**Usage Example**:
```rust
use robotech::db::init_db_conn;

// Initialize database connection
init_db_conn(config.clone()).await?;

// Auto migration
db_migrate!("postgres://localhost/mydb");

// Query
let entities = MyEntity::find().all(db).await?;
```

### 7. Signal Handling & Graceful Shutdown

Supports Unix signals for graceful shutdown:

**Supported Signals**:
- `SIGTERM` (15): Graceful shutdown
- `SIGINT` (2): Interrupt
- `SIGUSR1/USR2`: Custom handling

**Signal Commands**:
- `start`: Default, sends SIGCONT to check if already running
- `restart`: Sends SIGTERM to stop old process, then starts new one
- `stop/s`: Sends SIGTERM for graceful shutdown
- `kill/k`: Sends SIGKILL for forced termination

**Usage Example**:
```rust
use robotech::signal::SignalManager;

let (mut mgr, old_pid) = SignalManager::new("start".to_string())?;
let receiver = mgr.watch_signal()?;

// Wait for signal and gracefully close
wait_app_exit(signal_receiver, || async move {
    stop_services().await.ok();
    Ok(())
}).await?
```

### 8. Middleware System

Built-in common middleware:

| Middleware | Function | Description |
|------------|----------|-------------|
| CORS | Cross-origin resource sharing | Custom domains, methods, headers |
| Trace | Request tracing | Log request duration, status code |
| IpFilter | IP filtering | Whitelist/blacklist support |
| Health Check | Health monitoring | `/health` endpoint |
| Auth | Authentication | JWT/OAuth2 (customizable) |

**Adding Middleware**:
```rust
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

let app = Router::new()
    .route("/api/list", get(handler))
    .layer(TraceLayer::new_for_axum())
    .layer(CorsLayer::permissive());
```

## Quick Start

### Prerequisites

- Rust 1.75+ (2024 Edition)
- PostgreSQL 12+ or MySQL 8.0+ (if using database)
- Redis (optional, for caching)

### Step 1: Create Project

```bash
cargo new my-service
cd my-service
```

### Step 2: Add Dependencies

```toml
# Cargo.toml
[dependencies]
robotech = { version = "1.5.1", features = ["web", "db", "macros"] }
tokio = { version = "1", features = ["full"] }
```

### Step 3: Write Code

```rust
// main.rs
use robotech::app::AppConfig;
use robotech::db::init_db_conn;
use robotech::web::start_web_server;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct MyAppConfig {
    // Custom configuration
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize
    robotech::env::init_env()?;
    robotech::log::init_log()?;
    
    // 2. Load configuration
    let (config, _) = robotech::cfg::build_app_cfg::<AppConfig>(None)?;
    
    // 3. Database connection
    init_db_conn(config.db.clone()).await?;
    
    // 4. Start Web service
    start_web_server(config.web_server, None, None).await?;
    
    Ok(())
}
```

### Step 4: Configure Database

Create database and user:

```sql
-- PostgreSQL
CREATE USER myuser WITH PASSWORD 'mypassword';
CREATE DATABASE mydb OWNER myuser;

-- MySQL
CREATE USER 'myuser'@'%' IDENTIFIED BY 'mypassword';
CREATE DATABASE mydb CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
GRANT ALL PRIVILEGES ON mydb.* TO 'myuser'@'%';
```

### Step 5: Run Service

```bash
# Development mode
cargo run --features web,db

# Specify config file
cargo run --features web,db -c ./config.toml

# Release mode
cargo build --release
cargo run --features web,db -- -c ./config.toml
```

Access Swagger UI: http://localhost:9840/swagger-ui/

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
cargo build --features web,db
```

### Run Service

```bash
# Run service (requires web feature)
cargo run --features web

# Run service with database support
cargo run --features web,db
```

## Core Concepts

### RO (Response Object) - Response Object

All API responses are wrapped through `RO` structure, ensuring consistent return format.

### BaseDAO - Data Access Base Class

Generic encapsulation of common CRUD operations, enabling complete data access layer with just a few lines of code.

### EO (Entity Object) - Entity Object

Database table mapping via SeaORM Entity, auto-generated by `robotech-macros`.

### DTO/VO - Data Transfer/View Objects

- **DTO** (Data Transfer Object): Receive request parameters from frontend
- **VO** (View Object): Data structures returned to frontend

### Service - Business Logic Layer

Inherit `BaseSvc` or implement custom business logic, call DAO to complete data operations.

### Controller - Controller Layer

Handle HTTP requests, call Service layer, return RO response.

## Macro System (robotech-macros)

The project provides numerous macros to simplify development:

### Application Config Macros

- `build_app_cfg!` - Build application configuration
- `watch_cfg_file!` - Watch config file changes and hot-reload

### Database Macros

- `db_migrate!` - Execute database migration
- `db_conn!` - Get database connection

### Logging Macros

- `log_call` - Auto-record function call logs
- `log_result` - Record return values

### Controller Macros

- `impl_controller` - Auto-generate controller routes

Full documentation at: [robotech-macros](../robotech-macros/)

## Usage Examples

### Complete Microservice Example

```rust
use robotech::prelude::*;
use serde::{Deserialize, Serialize};
use sea_orm::EntityTrait;

// 1. Define configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceConfig {
    pub port: u16,
    pub db_url: String,
}

// 2. Define entity
#[derive(EntityTrait)]
struct User {
    #[pk]
    id: i64,
    name: String,
    email: String,
}

// 3. Implement DAO
impl_base_dao!(User);

// 4. Implement Service
struct UserService;

impl UserService {
    async fn create_user(ctx: &Context, data: CreateUserDto) -> Result<User> {
        // Business logic
        let user = User::create(ctx, data).await?;
        Ok(user)
    }
    
    async fn get_user_list(ctx: &Context) -> Result<Vec<User>> {
        let users = User::list(ctx).await?;
        Ok(users)
    }
}

// 5. Implement Controller
struct UserController;

#[async_trait]
impl UserController {
    async fn create(&self, State(ctx): State<Context>) -> impl IntoResponse {
        self.handle(|req: CreateUserDto| async move {
            let user = UserService::create_user(&ctx, req).await?;
            Ok(RO::success().detail(user).finish())
        })
        .await
    }
    
    async fn list(&self, State(ctx): State<Context>) -> impl IntoResponse {
        self.handle(|_| async move {
            let users = UserService::get_user_list(&ctx).await?;
            Ok(RO::success().detail(users).finish())
        })
        .await
    }
}

// 6. Start service
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_env()?;
    init_log()?;
    init_dao()?;
    
    let (config, _) = build_app_cfg::<ServiceConfig>(None)?;
    set_app_config(config.clone());
    
    db_migrate!(config.db_url.as_str());
    init_db_conn(DbConfig::from_url(&config.db_url)).await?;
    
    start_web_server(config.web_server, None, None).await?;
    
    Ok(())
}
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit issues and Pull Requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## Contact

- Project Homepage: https://github.com/rusthing/robotech-rs
- Issue Tracker: https://github.com/rusthing/robotech-rs/issues
- Author: zbz