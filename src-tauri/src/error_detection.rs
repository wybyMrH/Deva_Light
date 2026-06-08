pub fn text_looks_like_error(text: &str) -> bool {
    let value = text.to_ascii_lowercase();
    value.contains("unexpected status")
        || value.contains("bad gateway")
        || value.contains("gateway timeout")
        || value.contains("auth_not_found")
        || value.contains("no auth available")
        || value.contains("unauthorized")
        || value.contains("forbidden")
        || value.contains("connection refused")
        || value.contains("connection reset")
        || value.contains("connection error")
        || value.contains("failed to establish a new connection")
        || value.contains("retrying request after failure")
        || value.contains("retry failed")
        || value.contains("timed out")
        || value.contains("timeout")
        || value.contains("stream_error")
        || contains_http_error_code(&value)
}

fn contains_http_error_code(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| {
            part.len() == 3
                && matches!(part.as_bytes().first(), Some(b'4' | b'5'))
                && part.chars().all(|ch| ch.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gateway_auth_and_retry_failures() {
        assert!(text_looks_like_error(
            "unexpected status 502 Bad Gateway: auth_not_found: no auth available"
        ));
        assert!(text_looks_like_error(
            "Retrying request after failure (attempt 1 of 3)"
        ));
        assert!(text_looks_like_error("request failed with status 503"));
    }

    #[test]
    fn ignores_non_error_numbers() {
        assert!(!text_looks_like_error("completed 2026-06-08 in 120ms"));
        assert!(!text_looks_like_error("port 9200 is configured"));
    }
}
