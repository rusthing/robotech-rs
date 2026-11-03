#[cfg(feature = "db")]
use crate::dao::DaoError;
use crate::api::Ro;
#[cfg(feature = "db")]
use crate::api::{
    RO_CODE_WARNING_DELETE_VIOLATE_FK, RO_CODE_WARNING_DUPLICATE_KEY,
    RO_CODE_WARNING_INSERT_VIOLATE_FK,
};
use crate::svc::SvcError;
use axum::http::header::InvalidHeaderValue;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
#[cfg(feature = "db")]
use sea_orm::DbErr;
use thiserror::Error;
use tracing::warn;
use validator;

/// # 自定义控制器错误类型
///
/// 该枚举定义了控制器可能遇到的各种错误类型，包括运行时错误、参数校验错误、
/// IO 错误、非法 Header 值和服务层错误。
/// 通过实现 `IntoResponse` trait，这些错误可以被自动转换为 HTTP 响应。
///
/// 各变体说明：
/// - `Runtime(anyhow::Error)` - 运行时错误，通常返回 500 状态码
/// - `Validation(validator::ValidationError)` - 单个参数校验错误，通常返回 400 状态码
/// - `Validations(validator::ValidationErrors)` - 批量参数校验错误，通常返回 400 状态码
/// - `Io(std::io::Error)` - IO 操作错误，通常返回 500 状态码
/// - `InvalidHeaderValue(InvalidHeaderValue)` - 非法的 HTTP Header 值，通常返回 400 状态码
/// - `Svc(SvcError)` - 服务层错误，根据具体错误类型返回相应状态码
///
/// ## 示例
///
/// ```
/// use robotech::web::CtrlError;
///
/// let error = CtrlError::Io(std::io::Error::new(std::io::ErrorKind::Other, "磁盘故障"));
/// assert_eq!(format!("{}", error), "IO错误: 磁盘故障");
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
    #[error("Header值错误: {0}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
    #[error("服务层错误, {0}")]
    Svc(#[from] SvcError),
}

/// # 收集校验错误里的规则名（`code`）
///
/// 只取 `code`，不重复 `message`：`Display` 已经把「哪一项、期望什么」打出来了，
/// 这里补的是提示语里没有的那半截信息——到底哪条规则挂的
/// （`invalid_email`、`invalid_len`、`invalid_header_format`…），提示语相近时靠它区分。
///
/// 嵌套结构按 `a.b` 展开，列表按 `a[0].b` 展开，与 `ValidationErrors` 的 `Display`
/// 用同一套路径写法。
fn collect_error_codes(errors: &validator::ValidationErrors, prefix: &str, out: &mut Vec<String>) {
    for (field, kind) in errors.errors() {
        let path = if prefix.is_empty() {
            field.to_string()
        } else {
            format!("{prefix}.{field}")
        };
        match kind {
            validator::ValidationErrorsKind::Field(list) => {
                out.extend(list.iter().map(|error| format!("{path}: {}", error.code)));
            }
            validator::ValidationErrorsKind::Struct(inner) => {
                collect_error_codes(inner, &path, out)
            }
            validator::ValidationErrorsKind::List(map) => {
                for (idx, inner) in map {
                    collect_error_codes(inner, &format!("{path}[{idx}]"), out);
                }
            }
        }
    }
}

/// # 为 CtrlError 实现错误转换方法
///
/// 该实现定义了如何将不同类型的 控制器 错误转换为统一的 Ro 响应对象，以便在 HTTP 接口中返回标准化的错误信息格式
impl CtrlError {
    /// # 日志用的校验细节
    ///
    /// `Display` 只服务「回给调用方的那句话」，规则名（`code`）不在里面。
    /// 一句「参数<X-User-Id>格式不正确」看不出是哪条规则判的，所以校验类错误额外带上码：
    ///
    /// * 单个 `ValidationError`：`invalid_header_format`
    /// * `ValidationErrors`：`mobile: invalid_mobile_cn`（按字段路径逐条列出）
    ///
    /// 其它变体（数据库、IO…）的 `Display` 已经把关键信息说完了，不再额外展开。
    fn validation_detail(&self) -> Option<String> {
        match self {
            CtrlError::Validation(error) => Some(error.code.to_string()),
            CtrlError::Validations(errors) => {
                let mut codes = Vec::new();
                collect_error_codes(errors, "", &mut codes);
                Some(codes.join(", "))
            }
            CtrlError::Svc(SvcError::Validation(error)) => Some(error.code.to_string()),
            CtrlError::Svc(SvcError::Validations(errors)) => {
                let mut codes = Vec::new();
                collect_error_codes(errors, "", &mut codes);
                Some(codes.join(", "))
            }
            _ => None,
        }
    }

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
            CtrlError::InvalidHeaderValue(error) => {
                Ro::illegal_argument("Header值错误".to_string()).detail(Some(error.to_string()))
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
                    DaoError::DuplicateKey(unique_key, value) => {
                        Ro::warn(format!("{}<{}>已存在！", unique_key.key_remark, value))
                            .code(Some(RO_CODE_WARNING_DUPLICATE_KEY.to_string()))
                            .detail(Some(format!("{unique_key} -> value: {value}")))
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
        match self.validation_detail() {
            Some(detail) => warn!("控制器层捕获错误: {} | 校验规则: {}", self, detail),
            None => warn!("控制器层捕获错误: {}", self),
        }
        let status = match &self {
            CtrlError::Runtime(_) => StatusCode::INTERNAL_SERVER_ERROR,
            CtrlError::Validation(_)
            | CtrlError::Validations(_)
            | CtrlError::InvalidHeaderValue(_) => StatusCode::BAD_REQUEST,
            CtrlError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            CtrlError::Svc(error) => match error {
                SvcError::NotFound(_) => StatusCode::NOT_FOUND,
                SvcError::Validation(_)
                | SvcError::Validations(_)
                | SvcError::MultipartError(_) => StatusCode::BAD_REQUEST,
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

#[cfg(test)]
mod tests {
    use super::*;
    use validator::ValidationErrors;

    #[test]
    fn detail_lists_codes_per_field() {
        let mut errors = ValidationErrors::new();
        errors.add(
            "mobile",
            validator::ValidationError::new("invalid_mobile_cn"),
        );
        errors.add("email", validator::ValidationError::new("invalid_email"));

        let detail = CtrlError::Validations(errors)
            .validation_detail()
            .expect("校验类错误应有规则名");
        assert!(detail.contains("mobile: invalid_mobile_cn"), "{detail}");
        assert!(detail.contains("email: invalid_email"), "{detail}");
        // 提示语已经在 Display 里，日志里不再重复一遍
        assert!(!detail.contains("格式不正确"), "{detail}");
    }

    #[test]
    fn detail_is_just_the_code_for_single_error() {
        let error = CtrlError::Validation(validator::ValidationError::new("invalid_header_format"));
        assert_eq!(
            error.validation_detail().as_deref(),
            Some("invalid_header_format")
        );
    }

    #[test]
    fn other_errors_have_no_detail() {
        let error = CtrlError::Io(std::io::Error::other("磁盘故障"));
        assert!(error.validation_detail().is_none());
    }
}
