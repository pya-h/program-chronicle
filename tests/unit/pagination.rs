use crate::routes::payloads::common::PaginationQuery;

fn parse(value: serde_json::Value) -> PaginationQuery {
    serde_json::from_value(value).expect("valid pagination query")
}

#[test]
fn missing_fields_default_to_none() {
    let query = parse(serde_json::json!({}));
    assert_eq!(query.page, None);
    assert_eq!(query.per_page, None);
}

#[test]
fn numeric_strings_are_coerced() {
    // urlencoded flattened fields always arrive as strings, so the custom deserializer parses them
    let query = parse(serde_json::json!({"per_page": "50", "page": "2"}));
    assert_eq!(query.per_page, Some(50));
    assert_eq!(query.page, Some(2));
}

#[test]
fn empty_or_blank_string_is_treated_as_absent() {
    let query = parse(serde_json::json!({"per_page": "", "page": "  "}));
    assert_eq!(query.per_page, None);
    assert_eq!(query.page, None);
}

#[test]
fn non_numeric_string_is_rejected() {
    let result = serde_json::from_value::<PaginationQuery>(serde_json::json!({"per_page": "abc"}));
    assert!(result.is_err());
}

#[test]
fn out_of_range_value_is_rejected() {
    let result = serde_json::from_value::<PaginationQuery>(serde_json::json!({"per_page": "70000"}));
    assert!(result.is_err(), "70000 does not fit in a u16");
}
