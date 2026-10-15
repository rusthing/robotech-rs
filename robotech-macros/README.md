# robotech-macros

Procedural macros for the RoboTech platform.

## Features

- Code generation macros for RoboTech services
- Simplifies RESTful API development
- Integrates with the RoboTech framework

## Macros

| Macro | Kind | Purpose |
|-------|------|---------|
| `dic` | attribute | Represent a fieldless enum as an integer over HTTP and generate all its boilerplate |
| `crud_dto` | attribute | Generate DTO conversions |
| `vo` | attribute | Generate view objects |
| `router` | attribute | Generate CRUD routes |
| `log_call` | attribute | Log function calls |

### `dic`

RoboTech convention: **every dictionary enum must serialize to an integer**, never to a
variant-name string — and its whole boilerplate set is generated, not hand-written.

```rust
use robotech::macros::dic;

// No Default / From<i16> needed: omit the `default` parameter
#[dic]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum ConnectorType {
    Mysql = 0,
    Postgres = 1,
}

// Default / From<i16> needed: declare the default variant explicitly
#[dic(default = Unassigned)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum ConnectorStatus {
    Running = 0,
    Unassigned = 3,
}
```

The macro:

1. validates that the enum is non-generic, has at least one variant, that every variant is
   fieldless, and that each discriminant is an integer literal within `i16`;
2. strips `Serialize` / `Deserialize` / `ToSchema` / `Default` from `#[derive(...)]`, keeping all
   other derives;
3. always generates:
   - `Serialize` (as `i16`) and `Deserialize` (accepts `i16` only, **rejects invalid values**);
   - `PartialSchema` (OpenAPI `integer`, description `0=Manual, 1=Cron, ...`) and `ToSchema`;
   - `value()`, `from_i16() -> Option<Self>`, `From<X> for i16`, `From<X> for Option<i16>`;
   - `sea_orm::TryGetable` and `sea_orm::sea_query::ValueType` under `feature = "server"`,
     both **failing loudly** on invalid values;
4. **additionally** generates `Default` and `From<i16> for X`
   **only when `#[dic(default = VariantName)]` is given** — these are lossy conversions
   that all need a fallback variant. Without the parameter there is **no** `Default` impl,
   because "the first variant is the default" is usually just an accident of declaration order.
   `From<Option<i16>> for X` (`None` → `Default`) is deliberately **not** generated: it is the
   entry point where "a value that had no value becomes a value". Nullable columns must use
   `Option<Enum>` in the VO (see `robotech/README.md` → "Dictionary Enums (Dic)").

> `#[vo]` emits `#[from(~.into())]` for enum fields (`i16` → enum), and `From` cannot fail, so it
> needs `Default` to fall back on. **Every dictionary enum used as a VO field must therefore
> declare `default`.**

Domain methods (e.g. `AlertStatus::is_ended()`) are still written by hand in a separate
`impl EnumName { ... }` block.

See `robotech/README.md` → "Core Features 9. Dictionary Enums (Dic)" for the full convention.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
robotech-macros = "1.5.0"
```

## License

MIT
