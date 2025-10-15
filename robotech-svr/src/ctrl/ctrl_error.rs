use crate::svc::svc_error::SvcError;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use log::error;
use robotech_api::ro::ro::Ro;
use robotech_api::ro::ro_code::RO_CODE_WARNING_DELETE_VIOLATE_CONSTRAINT;
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
    #[error("参数校验错误: {0}")]
    ValidationError(#[from] validator::ValidationError),
    #[error("参数校验错误: {0}")]
    ValidationErrors(#[from] validator::ValidationErrors),
    #[error("IO错误: {0}")]
    IoError(#[from] std::io::Error),
    #[error("服务层错误")]
    SvcError(#[from] SvcError),
}

/// # 为 CtrlError 实现错误转换方法
///
/// 该实现定义了如何将不同类型的 控制器 错误转换为统一的 Ro 响应对象，以便在 HTTP 接口中返回标准化的错误信息格式
impl CtrlError {
    /// 将错误转换为Ro对象
    fn to_ro(&self) -> Ro<()> {
        match self {
            CtrlError::ValidationError(error) => {
                Ro::illegal_argument(format!("参数校验错误: {}", error.code))
            }
            CtrlError::ValidationErrors(errors) => {
                Ro::illegal_argument(format!("参数校验错误: {}", errors))
            }
            CtrlError::IoError(error) => {
                Ro::fail("磁盘异常".to_string()).detail(Some(error.to_string()))
            }
            CtrlError::SvcError(error) => match error {
                SvcError::NotFound(err) => {
                    Ro::warn("找不到数据".to_string()).detail(Some(err.to_string()))
                }
                SvcError::DuplicateKey(field_name, field_value) => {
                    Ro::warn(format!("{}<{}>已存在！", field_name, field_value))
                }
                SvcError::DeleteViolateConstraint(pk_table, foreign_key, fk_table) => {
                    Ro::warn("删除失败，有其它数据依赖于本数据".to_string())
                        .code(Some(RO_CODE_WARNING_DELETE_VIOLATE_CONSTRAINT.to_string()))
                        .detail(Some(format!(
                            "{} <- {} <- {}>",
                            pk_table, foreign_key, fk_table
                        )))
                }
                SvcError::DatabaseError(db_err) => match db_err {
                    DbErr::RecordNotUpdated => {
                        Ro::warn("未更新数据，请检查记录是否存在".to_string())
                    }
                    _ => Ro::fail("数据库错误".to_string()).detail(Some(db_err.to_string())),
                },
                _ => Ro::fail(error.to_string()),
            },
        }
    }
}
/// # 为 CtrlError 实现 ResponseError trait
/// 为 CtrlError 实现 ResponseError trait 以支持 Actix Web 的错误处理机制
///
/// 该实现定义了 控制器 错误如何转换为 HTTP 响应，包括状态码和响应体格式。
/// 根据不同的错误类型，会返回相应的 HTTP 状态码和格式化的错误信息。
impl ResponseError for CtrlError {
    /// 根据异常获取状态码
    fn status_code(&self) -> StatusCode {
        match self {
            CtrlError::ValidationError(_) | CtrlError::ValidationErrors(_) => {
                StatusCode::BAD_REQUEST
            }
            CtrlError::IoError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            CtrlError::SvcError(error) => match error {
                SvcError::NotFound(_) => StatusCode::NOT_FOUND,
                SvcError::DuplicateKey(_, _) | SvcError::DeleteViolateConstraint(_, _, _) => {
                    StatusCode::OK
                }
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
        }
    }

    /// 异常时响应的方法
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(self.to_ro())
    }
}
