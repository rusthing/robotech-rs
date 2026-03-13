use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub(super) struct MigrateArgs {
    db_url: Ident,
}

impl syn::parse::Parse for MigrateArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let db_url = input.parse()?;
        Ok(MigrateArgs { db_url })
    }
}

/// 声明宏：生成数据库迁移方法
///
/// # 使用示例
/// ```rust
/// // 基本用法（支持 MySQL、PostgreSQL、SQLite）
/// db_migrate!();
///
///
/// // 自定义方法名
/// db_migrate!(migrate_db);
///
/// // 指定 migrations 目录前缀
/// db_migrate!(migrate_db, "migrations");
/// ```
pub fn db_migrate_macro(args: MigrateArgs) -> TokenStream {
    let db_url = args.db_url;

    let expanded = quote! {
        use log::debug;
        use sqlx::any::install_default_drivers;
        use sqlx::AnyPool;

        debug!("migrating database...");
        install_default_drivers();
        let pool = AnyPool::connect(#db_url).await?;

        // 根据数据库类型选择迁移目录
        if db_url.starts_with("mysql://") {
            sqlx::migrate!("migrations/mysql")
        } else if db_url.starts_with("postgres://")
            || db_url.starts_with("postgresql://")
            || db_url.starts_with("postgis://")
        {
            sqlx::migrate!("migrations/pgsql")
        } else if db_url.starts_with("sqlite://") {
            sqlx::migrate!("migrations/sqlite")
        } else {
            return Err(anyhow!("不支持的数据库类型"));
        }
        .run(&pool).await.map_err(|e| anyhow!(format!("升级数据库版本时出错: {e}")))?;
    };

    TokenStream::from(expanded)
}
