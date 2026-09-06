use crate::core::error::SvmIndexerError;
use crate::routes::admin::common::{validate_http_url, validate_ws_url};

#[test]
fn http_url_passes_through_when_scheme_present() {
    assert_eq!(
        validate_http_url("https://rpc.example.com".to_string()).unwrap(),
        "https://rpc.example.com"
    );
    assert_eq!(
        validate_http_url("http://rpc.example.com".to_string()).unwrap(),
        "http://rpc.example.com"
    );
}

#[test]
fn http_url_gets_https_prefix_when_missing() {
    assert_eq!(
        validate_http_url("rpc.example.com".to_string()).unwrap(),
        "https://rpc.example.com"
    );
}

#[test]
fn http_url_rejects_non_http_scheme() {
    for input in ["ws://rpc.example.com", "ftp://rpc.example.com"] {
        let err = validate_http_url(input.to_string()).unwrap_err();
        assert!(
            matches!(err, SvmIndexerError::InvalidURL(_)),
            "{input} must be rejected"
        );
    }
}

#[test]
fn http_url_rejects_unparseable_input() {
    let err = validate_http_url("has spaces in it".to_string()).unwrap_err();
    assert!(matches!(err, SvmIndexerError::InvalidURL(_)));
}

#[test]
fn ws_url_passes_through_and_prefixes() {
    assert_eq!(
        validate_ws_url("wss://rpc.example.com".to_string()).unwrap(),
        "wss://rpc.example.com"
    );
    assert_eq!(
        validate_ws_url("ws://rpc.example.com".to_string()).unwrap(),
        "ws://rpc.example.com"
    );
    assert_eq!(
        validate_ws_url("rpc.example.com".to_string()).unwrap(),
        "wss://rpc.example.com"
    );
}

#[test]
fn ws_url_rejects_http_scheme() {
    let err = validate_ws_url("https://rpc.example.com".to_string()).unwrap_err();
    assert!(matches!(err, SvmIndexerError::InvalidURL(_)));
}
