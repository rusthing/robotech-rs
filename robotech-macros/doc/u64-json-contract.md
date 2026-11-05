# u64 字段的 JSON 契约

对外 JSON 里的 `u64`（主键、关联 id、毫秒时间戳、计数）统一输出字符串。原因是浏览器
`JSON.parse` 会把超过 `2^53-1` 的数字静默舍入；现网 idworker 的 19 位 id 已超过该上限。

## 一、宏结构体的写法

`#[vo]` / `#[crud_dto]` 结构体的字段写裸 `u64` / `Option<u64>`：

```rust
#[vo]
pub struct DtwRegionVo {
    pub id: u64,
    pub parent_id: Option<u64>,
}
```

宏在生成结构体时把字段类型改写成 `U64` / `Option<U64>`：

- `#[vo]`：`robotech-macros/src/vo/mod.rs` 的 `map_unsigned_type`；
- `#[crud_dto]`：`robotech-macros/src/dto/mod.rs` 的 `map_unsigned_type`。

不要在这些字段上手挂 `#[serde(with = ...)]`。

映射范围（HEAD）：`u8` / `u16` / `u32` / `u64` / `u128` 以及 `Option<同名类型>`。
`Vec<u64>`、`Option<Vec<u64>>`、`i64` 不在 `map_unsigned_type` 内，宏不会改写这些字段的
类型；宏结构体里的这类字段仍按原类型序列化。需要字符串数组时，当前没有框架级通道。

## 二、`U64` 的行为

- 序列化：`self.0.to_string()`，输出 JSON 字符串（`robotech/src/api/unsigned_integer.rs`
  的 `impl Serialize for U64`）。
- 反序列化：`impl_unsigned_de!(U64, u64)` 调 `tolerant_number::de`，接受字符串、
  JSON 数字（`u64` / `i64` / `u128` / `i128`）以及 `1.0` 这类浮点写法。
- 数字边界（HEAD）：`tolerant_number` 不做 `2^53` 检查，按 JSON 字面量收。字面量若来自
  浏览器，它已经是被 `JSON.parse` 舍入后的值。所以请求侧对 `u64` 一律发字符串。
- OpenAPI：`U64` 的 schema 是 `string | integer`（`impl_string_or_integer_schema!`）。

分页信封 `PageRx` 的 `page_num` / `total` 也是 `U64`，输出字符串。

## 三、手写结构体

不套宏的结构体（实时计算结果、聚合结果、跨服务载荷、导入结果等）没有 mo 实体，
`#[vo]` / `#[crud_dto]` 无法生成，按仓库既有写法给字段挂
`wheel_rs::serde::{u64_serde, u64_option_serde}`：

- `robotech/src/api/ro/ro.rs` 的 `timestamp`；
- `adm-api/src/vo/adm_auth_vo.rs` 的 `login_log_id`；
- `adm-api/src/vo/adm_dashboard_vo.rs`。

优先用宏的原因：宏一次管住请求与响应两个方向，不需要逐字段加属性；`#[vo]` 展开后本身也
`use wheel_rs::serde::{u64_serde, u64_option_serde}`，两条通道行为一致。

## 四、反例

不要为 `u64` 再建第三套序列化模块。曾经新增过 `robotech/src/api/json_number.rs`
（字段级 `u64_str` / `u64_str_opt` / `u64_str_vec`），与 `U64`、`wheel_rs::serde` 重复，
已删除。对外表示只保留两条通道：宏映射出的 `U64`，以及手写结构体上的
`wheel_rs::serde`。

## 五、浏览器侧后果

超过 `2^53-1` 的裸数字在 `JSON.parse` 阶段就被静默舍入（19 位 id 的末位会变成 0），
后续把该值回传只会命中错误对象或查不到。这个问题只能靠后端输出字符串解决；
`tolerant_number` 只作用于 Rust 反序列化，兜不住浏览器侧已经发生的精度丢失，
不要在这里加检查。
