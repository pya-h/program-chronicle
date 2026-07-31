use crate::core::error::SvmIndexerError;
use url::{ParseError, Url};

pub fn validate_http_url(url: String) -> Result<String, SvmIndexerError> {
    let http_url = if url.contains("://") {
        url
    } else {
        format!("https://{}", url).to_string()
    };

    if let Err(err) = Url::parse(&http_url).and_then(|h| {
        if matches!(h.scheme(), "http" | "https") {
            Ok(())
        } else {
            Err(ParseError::InvalidIpv4Address)
        }
    }) {
        return Err(SvmIndexerError::InvalidURL(format!(
            "Invalid http RPC provided: {}",
            err
        )));
    }
    Ok(http_url)
}

pub fn validate_ws_url(url: String) -> Result<String, SvmIndexerError> {
    let ws_url = if url.contains("://") {
        url
    } else {
        format!("wss://{}", url)
    };
    if let Err(err) = Url::parse(&ws_url).and_then(|w| {
        if matches!(w.scheme(), "ws" | "wss") {
            Ok(())
        } else {
            Err(ParseError::InvalidIpv4Address)
        }
    }) {
        return Err(SvmIndexerError::InvalidURL(format!(
            "Invalid web socket RPC provided! {}",
            err
        )));
    }
    Ok(ws_url)
}
