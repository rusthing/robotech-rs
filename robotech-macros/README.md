# robotech-macros

Procedural macros for the RoboTech platform.

## Features

- Code generation macros for RoboTech services
- Simplifies RESTful API development
- Integrates with the RoboTech framework

## Macros

| Macro | Kind | Purpose |
|-------|------|---------|
| `dic` | attribute | Represent a fieldless enum as an integer over HTTP and generate its OpenAPI schema |
| `crud_dto` | attribute | Generate DTO conversions |
| `vo` | attribute | Generate view objects |
| `router` | attribute | Generate CRUD routes |
| `log_call` | attribute | Log function calls |

### `dic`

RoboTech convention: **every dictionary enum must serialize to an integer**, never to a
variant-name string.

```rust
use robotech::macros::dic;

#[dic]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, AsRefStr, Display, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum JobType {
    Manual = 0,
    Cron = 1,
    FixedDelay = 2,
    FixedRate = 3,
}
```

The macro:

1. validates that the enum is non-generic, that every variant is fieldless, and that each
   discriminant is an integer literal within `i16`;
2. strips `Serialize` / `Deserialize` / `ToSchema` from `#[derive(...)]`, keeping all other derives;
3. generates `Serialize` (as `i16`), `Deserialize` (accepts `i16` only, rejects invalid values),
   `PartialSchema` (OpenAPI `integer`, description `0=Manual, 1=Cron, ...`) and `ToSchema`.

See `robotech/README.md` → "Core Features 9. Dictionary Enums (Dic)" for the full convention.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
robotech-macros = "1.5.0"
```

## License

MIT
