use robotech::macros::dao;

#[dao(
    unique_keys: [
        ("code", "编码"),
    ],
    like_columns: [
        Column::Code,
        Column::Name,
    ],
)]
pub struct TxItemDao;
