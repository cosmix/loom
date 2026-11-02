use super::super::assert_security_headers;
use crate::web_server::http;

#[test]
fn every_http_response_carries_the_security_headers() {
    for (status, reason) in [
        (200, "OK"),
        (403, "Forbidden"),
        (404, "Not Found"),
        (405, "Method Not Allowed"),
        (408, "Request Timeout"),
        (431, "Request Header Fields Too Large"),
        (500, "Internal Server Error"),
        (503, "Service Unavailable"),
    ] {
        let response =
            String::from_utf8(http::response_bytes(status, reason, "text/plain", b"body"))
                .expect("response bytes are UTF-8");
        assert_security_headers(&response);
        for directive in [
            "connect-src 'self';",
            "base-uri 'none';",
            "form-action 'none';",
            "frame-ancestors 'none'",
        ] {
            assert!(response.contains(directive), "missing {directive}");
        }
        assert!(
            !response.contains("ws://"),
            "connect-src must not name loopback WebSocket ports"
        );
    }
}
