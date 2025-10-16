//! # Response Object (RO) 模块，用于统一API响应格式

use crate::ro::ro_result::RoResult;
use chrono::Utc;
use serde::Serialize;
use std::fmt::Debug;
use utoipa::ToSchema;

/// # 统一API响应结构体
///
/// 用于封装所有API的返回结果，提供统一的响应格式
/// 包含结果状态、消息、时间戳以及可选的额外数据、详情和错误码
///
/// ## 泛型参数
/// * `E` - 额外数据的类型，用于携带具体的业务数据
#[derive(ToSchema, Debug, Serialize)]
pub struct Ro<E> {
    /// 响应结果枚举值，表示请求处理的结果状态
    pub result: RoResult,
    /// 响应消息，对结果的简要描述
    pub msg: String,
    /// 时间戳，记录响应生成的时间（毫秒）
    pub timestamp: u64,
    /// 额外数据，可选的响应数据内容
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<E>,
    /// 详细信息，可选的详细描述信息
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 编码，可选的业务编码
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl<E> Ro<E> {
    /// # 创建一个新的Ro实例
    ///
    /// ## 参数
    /// * `result` - 响应结果枚举值
    /// * `msg` - 响应消息
    ///
    /// ## 返回值
    /// 返回一个新的Ro实例
    pub fn new(result: RoResult, msg: String) -> Self {
        Ro {
            result,
            msg,
            timestamp: Utc::now().timestamp_millis() as u64,
            extra: None,
            detail: None,
            code: None,
        }
    }

    /// # 创建一个成功的响应对象
    ///
    /// ## 参数
    /// * `msg` - 成功消息
    ///
    /// ## 返回值
    /// 返回一个结果为Success的Ro实例
    pub fn success(msg: String) -> Self {
        Self::new(RoResult::Success, msg)
    }

    /// # 创建一个非法参数的响应对象
    ///
    /// ##  参数
    /// * `msg` - 错误消息
    ///
    /// ##  返回值
    /// 返回一个结果为IllegalArgument的Ro实例
    pub fn illegal_argument(msg: String) -> Self {
        Self::new(RoResult::IllegalArgument, msg)
    }

    /// # 创建一个警告的响应对象
    ///
    /// ## 参数
    /// * `msg` - 警告消息
    ///
    /// ## 返回值
    /// 返回一个结果为Warn的Ro实例
    pub fn warn(msg: String) -> Self {
        Self::new(RoResult::Warn, msg)
    }

    /// # 创建一个失败的响应对象
    ///
    /// ## 参数
    /// * `msg` - 失败消息
    ///
    /// ## 返回值
    /// 返回一个结果为Fail的Ro实例
    pub fn fail(msg: String) -> Self {
        Self::new(RoResult::Fail, msg)
    }

    /// # 设置响应消息
    ///
    /// ## 参数
    /// * `msg` - 新的消息内容
    ///
    /// ## 返回值
    /// 返回更新消息后的本实例
    pub fn msg(mut self, msg: String) -> Self {
        self.msg = msg;
        self
    }

    /// # 设置额外数据
    ///
    /// ## 参数
    /// * `extra` - 可选的额外数据
    ///
    /// ## 返回值
    /// 返回更新额外数据后的本实例
    pub fn extra(mut self, extra: Option<E>) -> Self {
        self.extra = extra;
        self
    }

    /// # 设置详细信息
    ///
    /// ## 参数
    /// * `detail` - 可选的详细信息
    ///
    /// ## 返回值
    /// 返回更新详细信息后的本实例
    pub fn detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }

    /// # 设置编码
    ///
    /// ## 参数
    /// * `code` - 可选的编码
    ///
    /// ## 返回值
    /// 返回更新编码后的本实例
    pub fn code(mut self, code: Option<String>) -> Self {
        self.code = code;
        self
    }

    /// # 获取当前实例的额外数据
    ///
    /// ## 返回值
    /// 返回包含额外数据的Option
    pub fn get_extra(self) -> Option<E> {
        self.extra
    }
}
