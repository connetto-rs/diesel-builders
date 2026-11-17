# validation-errors

[![Coverage](https://codecov.io/gh/connetto-rs/diesel-builders/branch/main/graph/badge.svg)](https://codecov.io/gh/connetto-rs/diesel-builders) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT) [![Docs](https://docs.rs/validation-errors/badge.svg)](https://docs.rs/validation-errors) [![Crates.io](https://img.shields.io/crates/v/validation-errors.svg)](https://crates.io/crates/validation-errors)

Validation errors for [`diesel-builders`](https://github.com/connetto-rs/diesel-builders) models. A `ValidationError` names the table and the failed rule, and converts into `diesel_builders::BuilderError` and `diesel::result::Error`, so a model declared with `#[table_model(error = ValidationError)]` can reject values in its `ValidateColumn` implementations.

```rust
use validation_errors::{ValidationError, ValidationErrorKind};

let error = ValidationError::smaller_than("events", "start_time", "end_time");
assert_eq!(error.table(), "events");
assert!(matches!(error.kind(), ValidationErrorKind::MustBeSmallerThan("start_time", "end_time")));
assert_eq!(
    error.to_string(),
    "Table `events`: Field `start_time` must be smaller than or equal to field `end_time`"
);
```
