# DTO 字段校验注释规范

`#[crud_dto]` 生成的 `XxxAddDto` 会按字段注释自动补 `validate` 校验。约束**只认注释
里的标签**，不从散文里猜：注释写「最长 100 字符」只是给人看的，标签 `@len` 才生效。

## 一、标签语法

标签单独成行，放在字段文档注释里：

```rust
/// 目标标识（用户 / 角色 / 分组 / 大屏的业务标识）
///
/// @len 0..=100 0 表示允许空串（发给该类别下全部目标）
pub target_id: String,
```

- 行首（去掉缩进后）必须是 `@`，紧跟标签名：`@len` 或 `@range`。
- 标签名后面跟一个区间，区间是**第一个空白分隔的词**；区间之后可以继续写说明，
  宏不解析说明文字。
- 区间写法（Rust 语法，和 `..=` 一致）：

| 写法 | 含义 |
| --- | --- |
| `1..=100` | 最小 1、最大 100 |
| `1..` | 最小 1、不设上限 |
| `..=100` | 不设下限、最大 100 |
| `0..=100` | **允许空串**、最大 100 |
| `0..` | 允许空串、不设上限（等于没有长度约束） |

- 数字可以是整数或小数，可带负号（`-90..=90`、`0.0..=1.0`）。
- `@len` 的下限写 `0` 就是「允许空串」的机器写法：不再生成 `min`，只留上限。
- 标签名拼错、区间写法不对、标签和字段类型不匹配 → **编译报错**，不会静默忽略。

## 二、两个标签

| 标签 | 适用字段 | 校验 |
| --- | --- | --- |
| `@len` | `String`、`Option<String>` | 字符数（不是字节数），生成 `length(min, max)` |
| `@range` | `i8` `i16` `i32` `i64` `f32` `f64`、`u8` `u16` `u32` `u64` `u128`（含 `Option`） | 数值区间，生成 `range(min, max)` |

无符号字段在 DTO 里会被包装成 `U8/U16/U32/U64/U128`，validator 只给原生整数实现了
`ValidateRange`。包装类型上的实现由 `robotech/src/api/unsigned_integer.rs` 补齐
（`ValidateRange<U16> for U16` 与 `for Option<U16>`），宏据此生成

```rust
#[validate(range(min = U16::from(1u16), max = U16::from(2u16), message = "…"))]
```

端点必须带类型后缀（`1u16` 而不是 `1`）：裸字面量会退化成 `i32`，`U16::from` 收不到。

也就是端点写普通数字、宏负责包成包装类型。这是 validator 支持自定义类型的标准做法：
**给自定义类型实现 `ValidateRange`（长度类字段则是 `ValidateLength`），再让它出现在
validate 属性里**。validator 自己的 `ValidateRangeType` 标记 trait 没有导出（`validation`
模块私有），所以走「直接实现 `ValidateRange`」，`Option<包装类型>` 那一半也只能手写，
因为 validator 的 blanket impl 要求 `T: ValidateRangeType`。

## 三、不写标签时的默认口径

| 字段类型 | 默认校验 |
| --- | --- |
| `String` | `required` + `length(min = 1)`：必须有值、不许空串，**不设上限** |
| `Option<String>` | 不加校验（`None` 合法、空串也合法） |
| 数值 | 只加 `required`（`Option` 数值不加） |
| 其它（时间、布尔、枚举……） | 只加 `required`（`Option` 不加）；枚举的值域另见第六节 |

所以「允许空串」是显式声明的结果：只有写了 `@len 0..=N`（或 `@len 0..`）才放开
最小长度，其余一律不许空串。

## 四、长度上限怎么取

上限取**数据库列定义的实际长度**，以迁移脚本为准：

| 列类型 | 注释 |
| --- | --- |
| `varchar(50)` | `@len 1..=50`（允许空串则 `@len 0..=50`） |
| `varchar(100)` / `varchar(200)` | 同上，按列长写 |
| `text` | `@len 1..`（只限非空，长度由数据库兜底） |
| `bigint` / `uuid` 等非字符串 | 不写 `@len` |

`Option<String>` 字段同样要写：`None` 合法，但一旦传了值，长度仍然受限。

## 五、数值字段的范围

只给**真有取值范围**的字段写 `@range`，ID、时间戳这类全量程字段不写：

```rust
/// 经度
/// @range -180..=180
pub geo_longitude: f64,

/// 纬度
/// @range -90..=90
pub geo_latitude: f64,
```

## 六、枚举字段（`#[dic]` 字典枚举）

枚举字段**不写标签**。`@len` / `@range` 用在枚举上会直接编译报错——这是故意的：
枚举没有长度，也不该用数字区间去描述它的值域。

那枚举字段靠什么校验？靠**类型本身**，在反序列化那一层就闭环了：

| 环节 | 越界值（比如 `{"jobType": 7}`）的处理 |
| --- | --- |
| `#[dic]` 生成的 `Deserialize` | 反序列化即报错，**不会**静默落成默认变体 |
| `#[dic]` 生成的 `TryGetable` / `ValueType` | 从库里读到越界值同样报错 |
| `#[crud_dto]` 生成的 `validate` | 只补 `required()`：非 `Option` 的枚举必须给值 |

所以「枚举字段有没有校验」的答案是：**有，而且比 validator 更早更严**。validator 里
再写一条 `range(min = 0, max = 3)` 是重复劳动，还会和枚举定义漂移——加个变体忘了改数字，
校验就成了假的。

**确需「这个接口只允许枚举的一部分变体」时**（例如新增任务只允许 `Running`），validator
没有内建的「取值必须在某个集合里」规则，只能在字段上手写 `custom`：

```rust
/// 任务状态：新增只允许运行中
#[validate(required(message = "任务状态不能为空"), custom(function = "check_task_status"))]
pub status: TaskStatus,
```

```rust
fn check_task_status(value: &TaskStatus) -> Result<(), validator::ValidationError> {
    if matches!(value, TaskStatus::Running) {
        Ok(())
    } else {
        let mut err = validator::ValidationError::new("status");
        err.message = Some("任务只能以运行中状态创建".into());
        Err(err)
    }
}
```

三个坑：

- **手写 `#[validate]` 会顶掉宏的自动校验**（宏只在字段没有 `validate` 时才补），所以
  `required()` 得自己写，否则 `Option` 里没值时这条路直接跳过；
- validator 对 `Option<T>` 字段会先解包再调用，`custom` 拿到的是 `&T`，不是 `&Option<T>`；
- 它还会把值塞进错误参数，字段类型要能 `Display`——字典枚举自带 `Display`，没问题。

这类规则是**接口级**的，不是类型级的，所以不进宏：同一个 `TaskStatus` 在查询接口上可以
取全部变体，在新增接口上只允许一个，宏看不到接口语义。

## 七、完整示例

```rust
#[crud_dto]
pub struct DtwControlPointDto {
    /// 经度
    /// @range -180..=180
    pub geo_longitude: f64,
    /// 纬度
    /// @range -90..=90
    pub geo_latitude: f64,
    /// 名称
    /// @len 1..=50
    pub name: String,
    /// 备注
    /// @len 0..=200
    pub remark: Option<String>,
}
```

生成的 `DtwControlPointAddDto` 字段上会是：

```rust
#[validate(
    required(message = "经度不能为空"),
    range(min = -180, max = 180, message = "经度取值范围是 -180~180")
)]
pub geo_longitude: Option<f64>,
```

## 八、校验提示语

提示语由宏按字段说明 + 区间自动拼，不需要手写：

- 必填：`{字段说明}不能为空`
- 长度：`{字段说明}长度需在 1~50 个字符之间` / `长度不能超过 N 个字符` / `长度不能少于 N 个字符`
- 区间：`{字段说明}取值范围是 -180~180` / `不能大于 N` / `不能小于 N`

字段说明取注释里第一条不是标签的行；没有说明就用字段名。

## 九、扫描与维护

改列长时，注释要一起改：**注释是唯一出处**，宏不读数据库。检查有没有漏：

```bash
grep -rn "pub .*: String" --include="*.rs" */\*-api/src/dto
```

逐个确认对应的 `@len` 是否与迁移脚本里的列定义一致。
