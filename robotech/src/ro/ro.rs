//! # Response Object (RO) 模块，用于统一API响应格式

use crate::ro::ro_result::RoResult;
use chrono::Utc;
use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::fmt::Debug;
use typed_builder::TypedBuilder;
use utoipa::ToSchema;

/// # 统一API响应结构体
///
/// 用于封装所有API的返回结果，提供统一的响应格式
/// 包含结果状态、消息、时间戳以及可选的额外数据、详情和错误码
///
/// ## 泛型参数
/// * `E` - 额外数据的类型，用于携带具体的业务数据
#[skip_serializing_none]
#[derive(ToSchema, Debug, Serialize, Deserialize, Setters, TypedBuilder)]
#[builder]
pub struct Ro<E> {
    /// 响应结果枚举值，表示请求处理的结果状态
    pub result: RoResult,
    /// 响应消息，对结果的简要描述
    pub message: String,
    /// 时间戳，记录响应生成的时间（毫秒）
    #[builder(default = Utc::now().timestamp_millis() as u64)]
    pub timestamp: u64,
    /// 额外数据，可选的响应数据内容
    #[builder(default, setter(strip_option))]
    pub extra: Option<E>,
    /// 详细信息，可选的详细描述信息
    #[builder(default, setter(strip_option))]
    pub detail: Option<String>,
    /// 编码，可选的业务编码
    #[builder(default, setter(strip_option))]
    pub code: Option<String>,
}

impl<E> Ro<E> {
    /// # 判断结果是否为成功
    ///
    /// ## 返回值
    /// 如果结果为Success，则返回true；否则返回false
    pub fn is_ok(&self) -> bool {
        self.result == RoResult::Success
    }

    /// # 判断结果是否有错误
    ///
    /// ## 返回值
    /// 如果结果不为Success，则返回true；否则返回false
    pub fn is_err(&self) -> bool {
        self.result != RoResult::Success
    }

    /// # 创建一个成功的响应对象
    ///
    /// ## 参数
    /// * `message` - 成功消息
    ///
    /// ## 返回值
    /// 返回一个结果为Success的Ro实例
    pub fn success(message: String) -> Self {
        Self::builder()
            .result(RoResult::Success)
            .message(message)
            .build()
    }

    /// # 创建一个非法参数的响应对象
    ///
    /// ## 参数
    /// * `message` - 错误消息
    ///
    /// ## 返回值
    /// 返回一个结果为IllegalArgument的Ro实例
    pub fn illegal_argument(message: String) -> Self {
        Self::builder()
            .result(RoResult::IllegalArgument)
            .message(message)
            .build()
    }

    /// # 创建一个警告的响应对象
    ///
    /// ## 参数
    /// * `message` - 警告消息
    ///
    /// ## 返回值
    /// 返回一个结果为Warn的Ro实例
    pub fn warn(message: String) -> Self {
        Self::builder()
            .result(RoResult::Warn)
            .message(message)
            .build()
    }

    /// # 创建一个失败的响应对象
    ///
    /// ## 参数
    /// * `message` - 失败消息
    ///
    /// ## 返回值
    /// 返回一个结果为Fail的Ro实例
    pub fn fail(message: String) -> Self {
        Self::builder()
            .result(RoResult::Fail)
            .message(message)
            .build()
    }
}
