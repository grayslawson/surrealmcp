use axum::extract::Request;
use axum::http::{Response, StatusCode};
use governor::middleware::NoOpMiddleware;
use metrics::counter;
use tower_governor::{
    GovernorLayer, errors::GovernorError, governor::GovernorConfigBuilder,
    key_extractor::KeyExtractor,
};
use tracing::{debug, warn};

/// Custom key extractor that tries to get IP from various headers and falls back to a default
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RobustIpKeyExtractor;

impl KeyExtractor for RobustIpKeyExtractor {
    type Key = String;

    fn extract<B>(&self, req: &Request<B>) -> Result<Self::Key, GovernorError> {
        // Output debugging information
        debug!(
            headers = ?req.headers(),
            "Attempting to extract IP address from request"
        );
        // Try to extract IP from various headers in order of preference
        let ip = req
            .headers()
            .get("X-Forwarded-For")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                req.headers()
                    .get("X-Real-IP") // Nginx
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                req.headers()
                    .get("X-Client-IP") // Proxies
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                req.headers()
                    .get("CF-Connecting-IP") // Cloudflare
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                req.headers()
                    .get("True-Client-IP") // Akamai
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                req.headers()
                    .get("X-Originating-IP")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                req.headers()
                    .get("X-Remote-IP")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                req.headers()
                    .get("X-Remote-Addr")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            });
        // If we find an idenfitying key, use it
        if let Some(ip) = ip {
            debug!(ip = ip, "Extracted IP address from headers");
            return Ok(ip.to_string());
        }
        // Otherwise, try to retrieve the connection info
        if let Some(addr) = req.extensions().get::<std::net::SocketAddr>() {
            debug!(ip = ?addr.ip(), "Extracted IP address from socket");
            return Ok(addr.ip().to_string());
        }
        // If we don't find an identifying key, use a default key
        warn!("Could not extract IP address from request, using default key");
        Ok("unknown".to_string())
    }
}

/// Create a rate limiting layer with metrics and logging
pub fn create_rate_limit_layer(
    rps: u32,
    burst: u32,
) -> GovernorLayer<RobustIpKeyExtractor, NoOpMiddleware, axum::body::Body> {
    // Output debugging information
    debug!("Configuring the HTTP rate limiter");
    // Create the rate limit configuration
    let config = GovernorConfigBuilder::default()
        .per_second(rps as u64)
        .burst_size(burst)
        .key_extractor(RobustIpKeyExtractor)
        .finish()
        .expect("Failed to create rate limit configuration");
    // Return the rate limit layer with error handler
    GovernorLayer::new(config).error_handler(|e| {
        // Output debugging information
        warn!("Rate limit exceeded: {e}");
        // Increment rate limit error metrics
        counter!("surrealmcp.total_errors").increment(1);
        counter!("surrealmcp.total_rate_limit_errors").increment(1);
        // Return the error response
        Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .body("Rate limit exceeded".into())
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn test_extract_x_forwarded_for_single() {
        let req = Request::builder()
            .header("X-Forwarded-For", "1.2.3.4")
            .body(Body::empty())
            .unwrap();
        let extractor = RobustIpKeyExtractor;
        let result = extractor.extract(&req).unwrap();
        assert_eq!(result, "1.2.3.4");
    }

    #[test]
    fn test_extract_x_forwarded_for_multiple() {
        let req = Request::builder()
            .header("X-Forwarded-For", "1.1.1.1, 2.2.2.2, 3.3.3.3")
            .body(Body::empty())
            .unwrap();
        let extractor = RobustIpKeyExtractor;
        let result = extractor.extract(&req).unwrap();
        assert_eq!(result, "1.1.1.1");
    }

    #[test]
    fn test_extract_x_forwarded_for_whitespace() {
        let req = Request::builder()
            .header("X-Forwarded-For", "  1.2.3.4  ")
            .body(Body::empty())
            .unwrap();
        let extractor = RobustIpKeyExtractor;
        let result = extractor.extract(&req).unwrap();
        assert_eq!(result, "1.2.3.4");
    }

    #[test]
    fn test_extract_ipv6() {
        let req = Request::builder()
            .header("X-Forwarded-For", "2001:db8::1")
            .body(Body::empty())
            .unwrap();
        let extractor = RobustIpKeyExtractor;
        let result = extractor.extract(&req).unwrap();
        assert_eq!(result, "2001:db8::1");
    }

    #[test]
    fn test_extract_various_headers() {
        let headers = [
            ("X-Real-IP", "5.6.7.8"),
            ("X-Client-IP", "1.2.3.4"),
            ("CF-Connecting-IP", "9.10.11.12"),
            ("True-Client-IP", "13.14.15.16"),
            ("X-Originating-IP", "17.18.19.20"),
            ("X-Remote-IP", "21.22.23.24"),
            ("X-Remote-Addr", "25.26.27.28"),
        ];

        for (name, val) in headers {
            let req = Request::builder()
                .header(name, val)
                .body(Body::empty())
                .unwrap();
            let extractor = RobustIpKeyExtractor;
            let result = extractor.extract(&req).unwrap();
            assert_eq!(result, val, "Failed for header {}", name);
        }
    }

    #[test]
    fn test_extract_fallback_to_socket_addr() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let mut req = Request::builder()
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(addr);

        let extractor = RobustIpKeyExtractor;
        let result = extractor.extract(&req).unwrap();
        assert_eq!(result, "127.0.0.1");
    }

    #[test]
    fn test_extract_fallback_to_unknown() {
        let req = Request::builder()
            .body(Body::empty())
            .unwrap();
        let extractor = RobustIpKeyExtractor;
        let result = extractor.extract(&req).unwrap();
        assert_eq!(result, "unknown");
    }

    #[test]
    fn test_extract_empty_header_fallback() {
        // Test that empty strings and whitespace-only strings fall through
        let cases = vec![
            ("", "1.2.3.4"),
            ("   ", "1.2.3.4"),
        ];

        for (val, expected) in cases {
            let req = Request::builder()
                .header("X-Forwarded-For", val)
                .header("X-Real-IP", expected)
                .body(Body::empty())
                .unwrap();
            let extractor = RobustIpKeyExtractor;
            let result = extractor.extract(&req).unwrap();
            assert_eq!(result, expected, "Failed to fall through for value '{}'", val);
        }
    }

    #[test]
    fn test_header_precedence() {
        let req = Request::builder()
            .header("X-Forwarded-For", "1.1.1.1")
            .header("CF-Connecting-IP", "2.2.2.2")
            .body(Body::empty())
            .unwrap();
        let extractor = RobustIpKeyExtractor;
        let result = extractor.extract(&req).unwrap();
        assert_eq!(result, "1.1.1.1");
    }
}
