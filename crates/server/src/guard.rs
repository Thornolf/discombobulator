use axum::extract::Request;
use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

pub const CSRF_HEADER: &str = "x-discombobulator";
const FORWARDED_HOST: &str = "x-forwarded-host";

fn hostname(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or_default();
    }
    authority.split(':').next().unwrap_or_default()
}

pub fn is_allowed_host(authority: &str) -> bool {
    let name = hostname(authority).to_ascii_lowercase();
    matches!(name.as_str(), "localhost" | "127.0.0.1" | "::1")
        || (name.ends_with(".ts.net")
            && name.len() > ".ts.net".len()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'))
}

fn header_text(headers: &HeaderMap, name: impl axum::http::header::AsHeaderName) -> Option<&str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn origin_authority(origin: &str) -> Option<&str> {
    origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .filter(|authority| !authority.contains('/'))
}

pub fn check(method: &Method, headers: &HeaderMap) -> Result<(), StatusCode> {
    let host = header_text(headers, HOST).ok_or(StatusCode::BAD_REQUEST)?;
    let forwarded = header_text(headers, FORWARDED_HOST);
    if !is_allowed_host(host) || forwarded.is_some_and(|value| !is_allowed_host(value)) {
        return Err(StatusCode::MISDIRECTED_REQUEST);
    }
    if matches!(*method, Method::GET | Method::HEAD) {
        return Ok(());
    }
    if header_text(headers, CSRF_HEADER) != Some("1") {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = header_text(headers, ORIGIN) {
        let authority = origin_authority(origin).ok_or(StatusCode::FORBIDDEN)?;
        let same_host = authority.eq_ignore_ascii_case(host)
            || forwarded.is_some_and(|value| authority.eq_ignore_ascii_case(value));
        if !same_host {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}

pub async fn middleware(request: Request, next: Next) -> Response {
    if let Err(status) = check(request.method(), request.headers()) {
        return status.into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        ("cross-origin-resource-policy", "same-origin"),
        (
            "content-security-policy",
            "default-src 'self'; img-src 'self' blob: data:; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'",
        ),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            if let Ok(value) = HeaderValue::from_str(value) {
                map.insert(*name, value);
            }
        }
        map
    }

    #[test]
    fn allows_local_and_tailnet_hosts_only() {
        assert!(is_allowed_host("127.0.0.1:47820"));
        assert!(is_allowed_host("localhost"));
        assert!(is_allowed_host("[::1]:47820"));
        assert!(is_allowed_host("desk.tail1234.ts.net"));
        assert!(!is_allowed_host("evil.com"));
        assert!(!is_allowed_host("127.0.0.1.evil.com"));
        assert!(!is_allowed_host("ts.net"));
        assert!(!is_allowed_host(".ts.net"));
        assert!(!is_allowed_host("192.168.1.10:47820"));
    }

    #[test]
    fn reads_are_allowed_without_csrf_header() {
        assert_eq!(
            check(&Method::GET, &headers(&[("host", "127.0.0.1:47820")])),
            Ok(())
        );
    }

    #[test]
    fn rebinding_host_is_rejected() {
        assert_eq!(
            check(
                &Method::GET,
                &headers(&[("host", "attacker.example:47820")])
            ),
            Err(StatusCode::MISDIRECTED_REQUEST)
        );
        assert_eq!(
            check(
                &Method::GET,
                &headers(&[
                    ("host", "127.0.0.1:47820"),
                    ("x-forwarded-host", "evil.com")
                ])
            ),
            Err(StatusCode::MISDIRECTED_REQUEST)
        );
    }

    #[test]
    fn writes_need_csrf_header_and_matching_origin() {
        let host = ("host", "127.0.0.1:47820");
        assert_eq!(
            check(&Method::POST, &headers(&[host])),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            check(&Method::POST, &headers(&[host, (CSRF_HEADER, "1")])),
            Ok(())
        );
        assert_eq!(
            check(
                &Method::PUT,
                &headers(&[
                    host,
                    (CSRF_HEADER, "1"),
                    ("origin", "http://127.0.0.1:47820")
                ])
            ),
            Ok(())
        );
        assert_eq!(
            check(
                &Method::PUT,
                &headers(&[host, (CSRF_HEADER, "1"), ("origin", "https://evil.ts.net")])
            ),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            check(
                &Method::PUT,
                &headers(&[host, (CSRF_HEADER, "1"), ("origin", "null")])
            ),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn tailnet_origin_matches_forwarded_host() {
        assert_eq!(
            check(
                &Method::POST,
                &headers(&[
                    ("host", "127.0.0.1:47820"),
                    ("x-forwarded-host", "desk.tail1234.ts.net"),
                    (CSRF_HEADER, "1"),
                    ("origin", "https://desk.tail1234.ts.net")
                ])
            ),
            Ok(())
        );
    }
}
