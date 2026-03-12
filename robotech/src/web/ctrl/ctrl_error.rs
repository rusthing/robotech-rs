use crate::dao::DaoError;
#[cfg(feature = "db")]
use crate::ro::RO_CODE_WARNING_DELETE_VIOLATE_FK;
use crate::ro::{Ro, RO_CODE_WARNING_INSERT_VIOLATE_FK};
use crate::svc::SvcError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use log::warn;
#[cfg(feature = "db")]
use sea_orm::DbErr;
use thiserror::Error;
use validator;

/// # 自定义控制器错误类型
///
/// 该枚举定义了控制器可能遇到的各种错误类型，包括参数校验错误、IO错误和服务层错误。
/// 通过实现ResponseError trait，这些错误可以被自动转换为HTTP响应。
///
/// ## Variants
///
/// * `ValidationError(String)` - 参数校验错误，通常返回400状态码
/// * `ValidationErrors(validator::ValidationErrors)` - 参数校验错误，通常返回400状态码
/// * `IoError(std::io::Error)` - IO操作错误，通常返回500状态码
/// * `SvcError(SvcError)` - 服务层错误，根据具体错误类型返回相应状态码
///
/// ## Examples
///
/// ```
/// use crate::utils::ctrl_utils::CtrlError;
///
/// let error = CtrlError::ValidationError("用户名不能为空".to_string());
/// assert_eq!(format!("{}", error), "参数校验错误: 用户名不能为空");
/// ```
#[derive(Debug, Error)]
pub enum CtrlError {
    #[error("{0}")]
    Runtime(#[from] anyhow::Error),
    #[error("参数校验错误 -> {0}")]
    Validation(#[from] validator::ValidationError),
    #[error("参数校验错误 -> {0}")]
    Validations(#[from] validator::ValidationErrors),
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("服务层错误, {0}")]
    Svc(#[from] SvcError),
}

/// # 为 CtrlError 实现错误转换方法
///
/// 该实现定义了如何将不同类型的 控制器 错误转换为统一的 Ro 响应对象，以便在 HTTP 接口中返回标准化的错误信息格式
impl CtrlError {
    /// 将错误转换为Ro对象
    fn to_ro(&self) -> Ro<()> {
        match self {
            CtrlError::Runtime(error) => {
                warn!("{}", error);
                Ro::warn("运行时错误".to_string()).detail(Some(error.to_string()))
            }
            CtrlError::Validation(error) => {
                Ro::illegal_argument(format!("参数校验错误 -> {}", error.to_string()))
            }
            CtrlError::Validations(errors) => {
                Ro::illegal_argument(format!("参数校验错误 -> {}", errors))
            }
            CtrlError::Io(error) => {
                Ro::fail("磁盘异常".to_string()).detail(Some(error.to_string()))
            }
            CtrlError::Svc(error) => match error {
                SvcError::Validation(error) => {
                    Ro::illegal_argument(format!("参数校验错误 -> {}", error.to_string()))
                }
                SvcError::Validations(errors) => {
                    Ro::illegal_argument(format!("参数校验错误 -> {}", errors))
                }
                SvcError::NotFound(err) => {
                    Ro::warn("找不到数据".to_string()).detail(Some(err.to_string()))
                }
                #[cfg(feature = "db")]
                SvcError::Dao(error) => match error {
                    DaoError::DuplicateKey(field_name, field_value) => {
                        Ro::warn(format!("{}<{}>已存在！", field_name, field_value))
                    }
                    DaoError::InsertViolateFk(foreign_key) => Ro::warn(format!(
                        "不能插入(或更新){}，设置的{}并不存在",
                        foreign_key.fk_table_comment, foreign_key.pk_table_comment
                    ))
                    .code(Some(RO_CODE_WARNING_INSERT_VIOLATE_FK.to_string()))
                    .detail(Some(foreign_key.to_string())),
                    DaoError::DeleteViolateFk(foreign_key) => Ro::warn(format!(
                        "不能删除(或更新){}，存在关联其的{}",
                        foreign_key.pk_table_comment, foreign_key.fk_table_comment
                    ))
                    .code(Some(RO_CODE_WARNING_DELETE_VIOLATE_FK.to_string()))
                    .detail(Some(foreign_key.to_string())),
                    DaoError::Db(db_err) => match db_err {
                        DbErr::RecordNotUpdated => {
                            Ro::warn("未更新数据，请检查记录是否存在".to_string())
                        }
                        _ => Ro::fail("数据库错误".to_string()).detail(Some(db_err.to_string())),
                    },
                    _ => Ro::fail("数据访问层错误".to_string()).detail(Some(error.to_string())),
                },
                _ => Ro::fail(error.to_string()),
            },
        }
    }
}

// 为错误类型实现 IntoResponse
impl IntoResponse for CtrlError {
    fn into_response(self) -> Response {
        warn!("控制器层捕获错误: {}", self);
        let status = match &self {
            CtrlError::Runtime(_) => StatusCode::INTERNAL_SERVER_ERROR,
            CtrlError::Validation(_) | CtrlError::Validations(_) => StatusCode::BAD_REQUEST,
            CtrlError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            CtrlError::Svc(error) => match error {
                SvcError::NotFound(_) => StatusCode::NOT_FOUND,
                #[cfg(feature = "db")]
                SvcError::Dao(error) => match error {
                    DaoError::DuplicateKey(_, _)
                    | DaoError::InsertViolateFk(_)
                    | DaoError::DeleteViolateFk(_) => StatusCode::OK,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                },
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
        };

        (status, Json(&self.to_ro())).into_response()
    }
}
