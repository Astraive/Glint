//! A deliberately small JSON Schema (2020-12) checker for the CLI output
//! envelope, so the test needs no schema crate.
//!
//! It implements exactly the keywords `schemas/glint-cli-output-v1.schema.json`
//! uses: local `$ref`, `oneOf`, `type`, `const`, `enum`, `required`,
//! `properties`, `items`, `additionalProperties: false`, `minLength` and `pattern`
//! (only the `^GLINT_[A-Z0-9_]+$` pattern). Any other keyword fails loudly,
//! so the schema cannot silently grow beyond what is checked.

use serde_json::Value;

const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "$comment",
    "$defs",
    "title",
    "description",
    "x-glint-status",
];

/// Validate `instance` against `schema` (the root document). Returns every
/// violation as `pointer: message`.
pub fn validate(root: &Value, instance: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    check(root, root, instance, "", &mut errors);
    errors
}

/// Validate `instance` against the sub-schema at the local reference
/// `reference` (e.g. `#/$defs/data/themeList`).
#[allow(dead_code)]
pub fn validate_at(root: &Value, reference: &str, instance: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    check(root, resolve(root, reference), instance, "", &mut errors);
    errors
}

fn resolve<'a>(root: &'a Value, reference: &str) -> &'a Value {
    let pointer = reference
        .strip_prefix('#')
        .unwrap_or_else(|| panic!("only local $ref is supported: {reference}"));
    root.pointer(pointer)
        .unwrap_or_else(|| panic!("unresolvable $ref {reference}"))
}

fn type_matches(name: &str, value: &Value) -> bool {
    match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        other => panic!("unknown type {other}"),
    }
}

fn check(root: &Value, schema: &Value, value: &Value, at: &str, errors: &mut Vec<String>) {
    let schema = schema
        .as_object()
        .unwrap_or_else(|| panic!("schema at {at} is not an object"));
    for (keyword, rule) in schema {
        match keyword.as_str() {
            k if ANNOTATIONS.contains(&k) => {}
            "$ref" => check(
                root,
                resolve(root, rule.as_str().unwrap()),
                value,
                at,
                errors,
            ),
            "oneOf" => {
                let passing = rule
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|branch| {
                        let mut branch_errors = Vec::new();
                        check(root, branch, value, at, &mut branch_errors);
                        branch_errors.is_empty()
                    })
                    .count();
                if passing != 1 {
                    errors.push(format!(
                        "{at}: matches {passing} oneOf branches, expected 1"
                    ));
                }
            }
            "type" => {
                let ok = match rule {
                    Value::String(name) => type_matches(name, value),
                    Value::Array(names) => names
                        .iter()
                        .any(|name| type_matches(name.as_str().unwrap(), value)),
                    _ => panic!("bad type keyword"),
                };
                if !ok {
                    errors.push(format!("{at}: expected type {rule}, got {value}"));
                }
            }
            "const" => {
                if rule != value {
                    errors.push(format!("{at}: expected {rule}, got {value}"));
                }
            }
            "enum" => {
                if !rule.as_array().unwrap().contains(value) {
                    errors.push(format!("{at}: {value} is not one of {rule}"));
                }
            }
            "required" => {
                if let Some(object) = value.as_object() {
                    for name in rule.as_array().unwrap() {
                        let name = name.as_str().unwrap();
                        if !object.contains_key(name) {
                            errors.push(format!("{at}: missing required `{name}`"));
                        }
                    }
                }
            }
            "properties" => {
                if let Some(object) = value.as_object() {
                    for (name, sub) in rule.as_object().unwrap() {
                        if let Some(child) = object.get(name) {
                            check(root, sub, child, &format!("{at}/{name}"), errors);
                        }
                    }
                }
            }
            "items" => {
                if let Some(array) = value.as_array() {
                    for (index, item) in array.iter().enumerate() {
                        check(root, rule, item, &format!("{at}/{index}"), errors);
                    }
                }
            }
            "additionalProperties" => {
                assert_eq!(
                    rule,
                    &Value::Bool(false),
                    "only additionalProperties: false"
                );
                let known = schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                if let Some(object) = value.as_object() {
                    for name in object.keys() {
                        if !known.contains_key(name) {
                            errors.push(format!("{at}: unexpected property `{name}`"));
                        }
                    }
                }
            }
            "minLength" => {
                if let Some(text) = value.as_str()
                    && (text.chars().count() as u64) < rule.as_u64().unwrap()
                {
                    errors.push(format!("{at}: shorter than {rule}"));
                }
            }
            "pattern" => {
                assert_eq!(rule, "^GLINT_[A-Z0-9_]+$", "unsupported pattern");
                if let Some(text) = value.as_str() {
                    let ok = text.strip_prefix("GLINT_").is_some_and(|rest| {
                        !rest.is_empty()
                            && rest
                                .chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    });
                    if !ok {
                        errors.push(format!("{at}: `{text}` does not match {rule}"));
                    }
                }
            }
            other => panic!("schema keyword `{other}` is not supported by the test checker"),
        }
    }
}
