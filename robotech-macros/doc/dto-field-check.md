# 字段校验注释规范（DTO 与 PowerDesigner 模型）

`#[crud_dto]` 生成的 `XxxAddDto` 会按字段注释自动补 `validate` 校验。约束**只认注释
里的标签**，不从散文里猜：注释写「最长 100 字符」只是给人看的，标签 `@len` 才生效。

同一套标签也要写进 PowerDesigner 模型的列注释里——模型是「从建表语句生成代码」的输入，
两边必须一致，见第十节。

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

## 二、两个标签（DTO 侧）

| 标签 | 适用字段 | 校验 |
| --- | --- | --- |
| `@len` | `String`、`Option<String>` | 字符数（不是字节数），生成 `length(min, max)` |
| `@range` | `i8` `i16` `i32` `i64` `f32` `f64`、`u8` `u16` `u32` `u64` `u128`（含 `Option`） | 数值区间，生成 `range(min, max)` |

第三个标签 `@enum` 只出现在模型里（第十节），DTO 侧不写。

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

枚举字段在 DTO 里**不写标签**。`@len` / `@range` 用在枚举上会直接编译报错——这是故意的：
枚举没有长度，也不该用数字区间去描述它的值域。

**用整数承载的枚举列要提炼出类型，不要用 `@range` 糊过去。** 列注释里一旦出现
`1-人` / `2-车` 这种值表，就说明这列有枚举语义：在 `xxx-api/src/dic/` 建 `#[dic]` 枚举、
DTO 与 VO 字段换成它、模型列注释改成 `@enum`（10.2、10.3）。`@range 1..=2` 只能说清
「范围是 1 到 2」，说不出「1 是谁、2 是谁」，生成的代码也只能停在 `u16`。

值表写在哪？写在**模型的列注释**里（`@enum`，见 10.2）：① 这是生成代码的输入，
② DTO 侧字段类型已经是枚举，不需要第二份。

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

改列长时，注释要一起改：改完 DTO 注释再用第十节的工具刷进模型。粗筛有没有漏：

```bash
grep -rn "pub .*: String" --include="*.rs" */\*-api/src/dto
```

逐个确认对应的 `@len` 是否与迁移脚本里的列定义一致。

## 十、模型里的注释（PowerDesigner）

标签的出处是**模型**，不只是 DTO：DTO 注释给 `#[crud_dto]` 看，模型注释给「从建表语句
生成代码」看。两者必须一致，否则生成的代码和手工维护的 DTO 会各说一套。

### 10.1 三个落点

| 文件 | 位置 |
| --- | --- |
| `model/*.ldm` | 逻辑模型实体属性 `Comment`（实体编码 = 表名去掉 `<模块>_` 前缀） |
| `model/*-pgsql.pdm`、`*-mysql.pdm` | 物理模型列 `Comment` |
| `model/create-table-*.sql` | `comment on column … is '…'`；MySQL 是列定义行内的 `comment '…'` |

三处内容相同：**散文 + 标签行**。散文是字段说明；`@len` / `@range` 单独成行、语法与
DTO 完全一致；枚举列再多一个 `@enum` 块（10.2，只写在模型里）。PowerDesigner 重新
生成 DDL 时注释原样带出，标签就一路流到建表语句。

### 10.2 `@enum`：枚举列怎么声明

枚举列用「头行 + 缩进变体」的块描述，读起来就是一段枚举定义。块内不留空行：

```
绑定目标类型
@enum BindTargetType
  1=Person   人
  2=Vehicle  车
```

变体多、带默认值时：

```
报警状态
@enum AlertStatus default=Temporary
  0=Temporary   临时(不是真正报警，用于计算防抖)
  1=InProgress  进行中
  2=Confirmed   已确认(用户接收到报警后先手工确认)
```

| 位置 | 写法 | 含义 |
| --- | --- | --- |
| 头行 | `@enum <类型名>` | 要生成的枚举类型名 |
| 头行 | `default=<变体名>` | 可选；有它才生成 `#[dic(default = …)]` |
| 变体行 | `  判别式=变体名` | 缩进两格，判别式照抄进 `= N` |
| 变体行 | 变体名后空**两格** | 之后都是变体说明，进 `///` 文档注释；可省 |

对齐只为读起来像代码：判别式右对齐、变体名左对齐、说明从同一列开始。同步工具生成的
块已经排好；手改时守住「变体名后至少两个空格」这个边界即可——解析靠它区分变体名和说明。

约定：

- `@enum` **只写在模型里**。DTO 字段类型已经是 `JobType`，再抄一遍值表就是第二份真相。
- 枚举列的散文**不再重复值表**，别写 `0: 手动分派 / 1: cron表达式`——值表只在 `@enum` 里。
- **整数承载的枚举列也必须提炼**，不能拿 `@range` 糊过去：`@range 1..=2` 只说得出取值范围，
  说不出「1 是谁、2 是谁」，生成的代码也只能停在 `u16`。值表一出现就建枚举（10.3）。
- 一个枚举被多列引用时，每列各写一个块，内容相同。
- 判别式必须是整数，范围 `i16`（与库里的 `INT2` 列一致）。

### 10.3 从 `@enum` 生成枚举类型

模型块到代码是逐行对应的，生成规则固定：

| 模型里 | 生成到代码 |
| --- | --- |
| `@enum BindTargetType` | `pub enum BindTargetType {` |
| `default=Person` | `#[dic(default = Person)]` |
| `1=Person   人` | `/// 人` + `Person = 1,` |
| 没写 `default=` | 不生成 `Default` / `From<i16>` |

```rust
use robotech::macros::dic;
use strum::{AsRefStr, Display, EnumString};

/// 定位卡绑定的目标类型，对应 `dtw_locator_bind.bind_target_type`。
#[dic(default = Person)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum BindTargetType {
    /// 人
    Person = 1,
    /// 车
    Vehicle = 2,
}
```

- 判别式照抄模型，不重排；变体说明原样进 `///`。
- 列可空 → DTO / VO 字段都写 `Option<枚举>`。被 `#[vo]` 用到的枚举**必须**有 `default`
  （`#[vo]` 生成 `~.map(|v| v.into())`，需要 `From<i16>`）。
- 枚举落在 `xxx-api/src/dic/`，`mod.rs` 里 `mod` + `pub use`；`lib.rs` 加 `pub mod dic;`。
- 生成完把 DTO / VO 原来的 `u16` / `Option<u16>` 换成枚举，然后 `--check` 确认模型与代码对齐。

### 10.4 同步与校验

```bash
# 只报告差异（有差异退出码为 1，适合放在提交前）
python3 robotech-rs/tools/model_comment/sync.py --repo ../msg --check

# 写回 LDM / PDM / create-table-*.sql
python3 robotech-rs/tools/model_comment/sync.py --repo ../msg --write
```

- `--repo` 可重复，一次刷多个仓库。
- `--write` 幂等：标签行整行替换、散文原样保留，CRLF 的模型文件不会被整体改写。
- 判据是「DTO 的标签 == 模型里的标签」。模型多出来的标签报成不一致，**不会自动删**。
- 只处理 DTO 里出现过的列；`[模型缺列]` 表示模型漏了表或列（新表还没进模型）。
- 新增列时的顺序是：建表脚本 / 模型 → DTO 注释 → 跑 `--check` 确认 0 处不一致。

### 10.5 已知边界

- `@len` 的上限与列长度没有交叉校验（`varchar(50)` 抄成 `@len 1..=500` 不会被发现），
  改列长时两处一起改，再跑一次 `--check`。
- 模型里没有 DTO 的列（系统列、未暴露的列）不写标签——它们不参与参数校验。
