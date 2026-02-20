# db_unwrap 属性宏使用说明

## 概述

`db_unwrap` 是一个属性宏，用于简化 Service 层查询方法的编写。它会自动处理数据库连接逻辑（`if let Some(db) = db` 和 `else` 分支），让开发者只需要专注于核心的业务逻辑和返回语句。

## 功能特性

- 自动处理 `db: Option<&C>` 参数的两种情况（有连接 vs 无连接）
- 自动生成数据库连接获取逻辑
- 减少重复的样板代码
- 保持完整的业务逻辑控制权
- 提高代码可读性和维护性

## 使用方法

### 基本用法

```rust
use robotech_macros::db_unwrap;

#[db_unwrap]
pub async fn get_by_name<C>(name: &str, db: Option<&C>) -> Result<Ro<OssBucketVo>, SvcError>
where
    C: ConnectionTrait,
{
    // 只需要写核心业务逻辑和返回语句
    let one = OssBucketDao::get_by_name(name, db).await?;
    Ok(
        Ro::success("查询成功".to_string())
            .extra(one.map(|value| OssBucketVo::from(value))),
    )
}
```

## 宏展开后的代码

使用宏的方法会展开为完整的传统写法：

```rust
pub async fn get_by_name<C>(name: &str, db: Option<&C>) -> Result<Ro<OssBucketVo>, SvcError>
where
    C: ConnectionTrait,
{
    if let Some(db) = db {
        let one = OssBucketDao::get_by_name(name, db).await?;
        Ok(
            Ro::success("查询成功".to_string())
                .extra(one.map(|value| OssBucketVo::from(value))),
        )
    } else {
        let db_conn = robotech::db_conn::get_db_conn()?;
        let db = db_conn.as_ref();
        let one = OssBucketDao::get_by_name(name, db).await?;
        Ok(
            Ro::success("查询成功".to_string())
                .extra(one.map(|value| OssBucketVo::from(value))),
        )
    }
}
```

## 代码对比

### 使用宏后（简化写法）

```rust
#[db_unwrap]
pub async fn get_by_name<C>(name: &str, db: Option<&C>) -> Result<Ro<OssBucketVo>, SvcError>
where
    C: ConnectionTrait,
{
    let one = OssBucketDao::get_by_name(name, db).await?;
    Ok(
        Ro::success("查询成功".to_string())
            .extra(one.map(|value| OssBucketVo::from(value))),
    )
}
```

### 使用宏前（传统写法）

```rust
pub async fn get_by_name<C>(name: &str, db: Option<&C>) -> Result<Ro<OssBucketVo>, SvcError>
where
    C: ConnectionTrait,
{
    if let Some(db) = db {
        let one = OssBucketDao::get_by_name(name, db).await?;
        Ok(
            Ro::success("查询成功".to_string())
                .extra(one.map(|value| OssBucketVo::from(value))),
        )
    } else {
        let db_conn = get_db_conn()?;
        let db = db_conn.as_ref();
        let one = OssBucketDao::get_by_name(name, db).await?;
        Ok(
            Ro::success("查询成功".to_string())
                .extra(one.map(|value| OssBucketVo::from(value))),
        )
    }
}
```

## 优势

1. **代码量减少**: 从 17 行减少到 7 行，减少了约 60% 的样板代码
2. **错误减少**: 自动处理数据库连接逻辑，减少人为错误
3. **维护性**: 统一的代码结构，便于维护和重构
4. **可读性**: 突出核心业务逻辑，提高代码可读性
5. **灵活性**: 用户仍然完全控制业务逻辑和返回格式

## 注意事项

1. 方法必须包含 `db: Option<&C>` 参数
2. 用户需要编写完整的业务逻辑，包括查询和返回语句
3. 宏只负责处理数据库连接的外层逻辑
4. 用户代码中的 `db` 参数在宏展开后会是正确的类型（`&C` 而不是 `Option<&C>`）

## 依赖要求

- 需要在 `Cargo.toml` 中添加 `robotech-macros` 依赖
- 需要启用 `robotech` 的 `macros` 特性

```toml
[dependencies]
robotech = { workspace = true, features = ["web", "db", "macros"] }
robotech-macros = { path = "../../robotech-rs/robotech-macros" }
```

## 实际应用场景

这个宏特别适合于：
- Service 层的查询方法
- 需要处理可选数据库连接的方法
- 标准的 CRUD 操作中的查询部分
- 需要统一数据库连接处理逻辑的项目
