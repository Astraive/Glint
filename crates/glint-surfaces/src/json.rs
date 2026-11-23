//! Small typed readers over `serde_json` values that produce
//! [`SurfaceError`]s with JSON Pointers.

use serde_json::{Map, Value};

use crate::error::{SurfaceError, SurfaceErrorCode, child_pointer};

pub(crate) type Result<T> = std::result::Result<T, SurfaceError>;

/// A JSON object together with its pointer.
pub(crate) struct Obj<'a> {
    pub(crate) map: &'a Map<String, Value>,
    pub(crate) pointer: String,
}

impl<'a> Obj<'a> {
    pub(crate) fn new(value: &'a Value, pointer: String) -> Result<Self> {
        match value {
            Value::Object(map) => Ok(Self { map, pointer }),
            other => Err(type_error(&pointer, "an object", other)),
        }
    }

    pub(crate) fn child(&self, key: &str) -> String {
        child_pointer(&self.pointer, key)
    }

    /// Rejects the first key (in sorted order) not in `allowed`.
    pub(crate) fn deny_unknown(&self, allowed: &[&str]) -> Result<()> {
        let mut keys: Vec<&String> = self.map.keys().collect();
        keys.sort();
        match keys.into_iter().find(|k| !allowed.contains(&k.as_str())) {
            Some(key) => Err(SurfaceError::new(
                SurfaceErrorCode::UnknownField,
                self.child(key),
                format!(
                    "{key:?} is not a known field here; expected one of: {}",
                    allowed.join(", ")
                ),
            )),
            None => Ok(()),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<&'a Value> {
        self.map.get(key)
    }

    pub(crate) fn optional_str(&self, key: &str) -> Result<Option<&'a str>> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(other) => Err(type_error(&self.child(key), "a string", other)),
        }
    }

    pub(crate) fn optional_bool(&self, key: &str) -> Result<Option<bool>> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(other) => Err(type_error(&self.child(key), "a boolean", other)),
        }
    }

    /// An optional integer in `min..=max`.
    pub(crate) fn optional_int(&self, key: &str, min: u32, max: u32) -> Result<Option<u32>> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        let pointer = self.child(key);
        let Value::Number(number) = value else {
            return Err(type_error(&pointer, "an integer", value));
        };
        let range = || {
            SurfaceError::new(
                SurfaceErrorCode::OutOfRange,
                pointer.clone(),
                format!("{number} is outside {min}..={max}"),
            )
        };
        if let Some(n) = number.as_u64() {
            let n = u32::try_from(n).map_err(|_| range())?;
            if (min..=max).contains(&n) {
                Ok(Some(n))
            } else {
                Err(range())
            }
        } else if number.is_i64() {
            Err(range())
        } else {
            Err(type_error(&pointer, "an integer", value))
        }
    }
}

pub(crate) fn type_error(pointer: &str, expected: &str, found: &Value) -> SurfaceError {
    let found = match found {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    };
    SurfaceError::new(
        SurfaceErrorCode::InvalidType,
        pointer,
        format!("expected {expected}, found {found}"),
    )
}

/// Parses a string field against a vocabulary of `(name, value)` pairs.
pub(crate) fn vocabulary<T: Copy>(
    pointer: &str,
    text: &str,
    words: &[(&'static str, T)],
) -> Result<T> {
    words
        .iter()
        .find(|(name, _)| *name == text)
        .map(|(_, value)| *value)
        .ok_or_else(|| {
            let names: Vec<&str> = words.iter().map(|(name, _)| *name).collect();
            SurfaceError::new(
                SurfaceErrorCode::InvalidValue,
                pointer,
                format!("{text:?} is not one of: {}", names.join(", ")),
            )
        })
}

pub(crate) fn conflict(pointer: impl Into<String>, message: impl Into<String>) -> SurfaceError {
    SurfaceError::new(SurfaceErrorCode::Conflict, pointer, message)
}
