use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use chrono::{DateTime, SecondsFormat, Utc};

/// GraphQL scalar matching the Node backend's `DateTimeISO` (from
/// `graphql-scalars`): an ISO-8601 / RFC 3339 timestamp. The scalar is named
/// `DateTimeISO` on purpose so the emitted SDL is byte-compatible with the Node
/// contract and the existing SPA needs no query changes — async-graphql's
/// built-in chrono support would instead name it `DateTime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTimeIso(pub DateTime<Utc>);

#[Scalar(name = "DateTimeISO")]
impl ScalarType for DateTimeIso {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(raw) => {
                let parsed = DateTime::parse_from_rfc3339(&raw)
                    .map_err(InputValueError::custom)?
                    .with_timezone(&Utc);
                Ok(DateTimeIso(parsed))
            }
            other => Err(InputValueError::expected_type(other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}

impl From<DateTime<Utc>> for DateTimeIso {
    fn from(value: DateTime<Utc>) -> Self {
        DateTimeIso(value)
    }
}
