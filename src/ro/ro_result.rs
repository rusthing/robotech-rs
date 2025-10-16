//! # RoResult 枚举定义了API响应的结果状态
//!
//! 该模块定义了统一的API响应结果类型，包括成功、参数错误、警告和失败四种状态

use serde::{Deserialize, Serialize};
use std::fmt;
use utoipa::ToSchema;

/// # API响应结果枚举
///
/// 定义了四种可能的API响应结果状态：
/// - Success: 操作成功
/// - IllegalArgument: 参数错误
/// - Warn: 警告状态
/// - Fail: 操作失败
#[derive(ToSchema, Debug, Copy, Clone)]
pub enum RoResult {
    Success,
    IllegalArgument,
    Warn,
    Fail,
}

/// # 枚举元数据结构
///
/// 用于存储每个枚举值的详细信息，包括ID、名称和说明
struct EnumMetadata {
    /// 枚举值的ID，用于序列化/反序列化
    id: i8,
    /// 枚举值的中文名称
    name: &'static str,
    /// 枚举值的说明信息
    note: &'static str,
}

/// # 枚举元数据常量数组
///
/// 按照枚举值在定义中的顺序存储每个枚举值的元数据信息
const ENUM_METADATA: [EnumMetadata; 4] = [
    EnumMetadata {
        id: 1,
        name: "成功",
        note: "运行正常",
    },
    EnumMetadata {
        id: -1,
        name: "参数错误",
        note: "传递的参数有问题",
    },
    EnumMetadata {
        id: -2,
        name: "警告",
        note: "用户方面的错误",
    },
    EnumMetadata {
        id: -3,
        name: "失败",
        note: "系统方面的异常",
    },
];

impl RoResult {
    /// # 获取枚举值对应的元数据
    ///
    /// ## 返回值
    /// 返回指向对应元数据的引用
    fn get_metadata(&self) -> &EnumMetadata {
        &ENUM_METADATA[*self as usize]
    }

    /// # 根据ID获取对应的枚举对象
    ///
    /// ## 参数
    /// * `id` - 枚举值的ID
    ///
    /// ## 返回值
    /// 如果找到对应的枚举值，返回Some(RoResult)，否则返回None
    pub fn new(id: i8) -> Option<Self> {
        ENUM_METADATA
            .iter()
            .position(|metadata| metadata.id == id)
            .map(|index| unsafe { std::mem::transmute(index as u8) })
    }
}

impl fmt::Display for RoResult {
    /// # 格式化输出枚举值信息
    ///
    /// ## 参数
    /// * `f` - 格式化器
    ///
    /// ## 返回值
    /// 格式化结果
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let metadata = self.get_metadata();
        write!(
            f,
            "RoResult {{ index: {}, id: {}, name: {}, note: {} }}",
            *self as usize, metadata.id, metadata.name, metadata.note
        )
    }
}

impl Serialize for RoResult {
    /// # 序列化枚举值为JSON
    ///
    /// ## 参数
    /// * `serializer` - 序列化器
    ///
    /// ## 返回值
    /// 序列化结果
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i8(self.get_metadata().id)
    }
}

impl<'de> Deserialize<'de> for RoResult {
    /// # 从JSON反序列化为枚举值
    ///
    /// ## 参数
    /// * `deserializer` - 反序列化器
    ///
    /// ## 返回值
    /// 反序列化结果，如果ID无效则返回错误
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = i8::deserialize(deserializer)?;
        RoResult::new(id)
            .ok_or_else(|| serde::de::Error::custom(format!("Unknown RoResult id: {}", id)))
    }
}
