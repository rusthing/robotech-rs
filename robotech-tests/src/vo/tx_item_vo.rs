use robotech::macros::vo;

#[vo]
pub struct TxItemVo {
    /// ID
    pub id: u64,
    /// 编码
    pub code: String,
    /// 名称
    pub name: Option<String>,
    /// 创建者ID
    pub creater_id: u64,
    /// 创建时间戳
    pub create_ms: u64,
    /// 更新者ID
    pub updater_id: u64,
    /// 更新时间戳
    pub update_ms: u64,
}
