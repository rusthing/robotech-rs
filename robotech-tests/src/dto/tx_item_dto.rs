use robotech::macros::crud_dto;

#[crud_dto]
pub struct TxItemDto {
    /// 编码
    /// @len 1..=50
    pub code: String,
    /// 名称
    /// @len 0..=50
    pub name: Option<String>,
}
