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

fn is_image_route(path: &str) -> bool {
    path.strip_prefix("/api/photos/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(_, variant)| variant == "thumb" || variant == "preview")
}

pub fn check(
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    tailnet_host: Option<&str>,
) -> Result<(), StatusCode> {
    let host = header_text(headers, HOST).ok_or(StatusCode::BAD_REQUEST)?;
    let forwarded = header_text(headers, FORWARDED_HOST);
    if !is_allowed_host(host) || forwarded.is_some_and(|value| !is_allowed_host(value)) {
        return Err(StatusCode::MISDIRECTED_REQUEST);
    }
    let reading = matches!(*method, Method::GET | Method::HEAD);
    if reading && !path.starts_with("/api/") {
        return Ok(());
    }
    if reading && is_image_route(path) {
        return match header_text(headers, "sec-fetch-site") {
            None | Some("same-origin" | "none") => Ok(()),
            Some(_) => Err(StatusCode::FORBIDDEN),
        };
    }
    if header_text(headers, CSRF_HEADER) != Some("1") {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = header_text(headers, ORIGIN) {
        let authority = origin_authority(origin).ok_or(StatusCode::FORBIDDEN)?;
        let same_host = authority.eq_ignore_ascii_case(host)
            || forwarded.is_some_and(|value| authority.eq_ignore_ascii_case(value))
            || (origin.starts_with("https://")
                && tailnet_host.is_some_and(|name| authority.eq_ignore_ascii_case(name)));
        if !same_host {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}

pub async fn middleware(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let tailnet = state.tailnet_host.read().ok().and_then(|name| name.clone());
    if let Err(status) = check(
        request.method(),
        request.uri().path(),
        request.headers(),
        tailnet.as_deref(),
    ) {
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
    fn api_reads_need_the_csrf_header_but_pages_do_not() {
        let host = ("host", "127.0.0.1:47820");
        assert_eq!(check(&Method::GET, "/", &headers(&[host]), None), Ok(()));
        assert_eq!(
            check(&Method::GET, "/api/folders", &headers(&[host]), None),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            check(
                &Method::GET,
                "/api/folders",
                &headers(&[host, (CSRF_HEADER, "1")]),
                None
            ),
            Ok(())
        );
    }

    #[test]
    fn images_refuse_cross_site_loads() {
        let host = ("host", "127.0.0.1:47820");
        let path = "/api/photos/0123456789abcdef0123/thumb";
        assert_eq!(check(&Method::GET, path, &headers(&[host]), None), Ok(()));
        assert_eq!(
            check(
                &Method::GET,
                path,
                &headers(&[host, ("sec-fetch-site", "same-origin")]),
                None
            ),
            Ok(())
        );
        assert_eq!(
            check(
                &Method::GET,
                path,
                &headers(&[host, ("sec-fetch-site", "cross-site")]),
                None
            ),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn pages_are_allowed_without_csrf_header() {
        assert_eq!(
            check(
                &Method::GET,
                "/index.html",
                &headers(&[("host", "127.0.0.1:47820")]),
                None
            ),
            Ok(())
        );
    }

    #[test]
    fn rebinding_host_is_rejected() {
        assert_eq!(
            check(
                &Method::GET,
                "/api/library",
                &headers(&[("host", "attacker.example:47820")]),
                None
            ),
            Err(StatusCode::MISDIRECTED_REQUEST)
        );
        assert_eq!(
            check(
                &Method::GET,
                "/api/library",
                &headers(&[
                    ("host", "127.0.0.1:47820"),
                    ("x-forwarded-host", "evil.com")
                ]),
                None
            ),
            Err(StatusCode::MISDIRECTED_REQUEST)
        );
    }

    #[test]
    fn writes_need_csrf_header_and_matching_origin() {
        let host = ("host", "127.0.0.1:47820");
        assert_eq!(
            check(&Method::POST, "/api/library", &headers(&[host]), None),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            check(
                &Method::POST,
                "/api/library",
                &headers(&[host, (CSRF_HEADER, "1")]),
                None
            ),
            Ok(())
        );
        assert_eq!(
            check(
                &Method::PUT,
                "/api/library",
                &headers(&[
                    host,
                    (CSRF_HEADER, "1"),
                    ("origin", "http://127.0.0.1:47820")
                ]),
                None
            ),
            Ok(())
        );
        assert_eq!(
            check(
                &Method::PUT,
                "/api/library",
                &headers(&[host, (CSRF_HEADER, "1"), ("origin", "https://evil.ts.net")]),
                None
            ),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            check(
                &Method::PUT,
                "/api/library",
                &headers(&[host, (CSRF_HEADER, "1"), ("origin", "null")]),
                None
            ),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn tailnet_origin_matches_the_machine_name() {
        let headers = headers(&[
            ("host", "127.0.0.1:47820"),
            (CSRF_HEADER, "1"),
            ("origin", "https://desk.tail1234.ts.net"),
        ]);
        assert_eq!(
            check(
                &Method::POST,
                "/api/decisions",
                &headers,
                Some("desk.tail1234.ts.net")
            ),
            Ok(())
        );
        assert_eq!(
            check(
                &Method::POST,
                "/api/decisions",
                &headers,
                Some("other.tail1234.ts.net")
            ),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn tailnet_origin_matches_forwarded_host() {
        assert_eq!(
            check(
                &Method::POST,
                "/api/library",
                &headers(&[
                    ("host", "127.0.0.1:47820"),
                    ("x-forwarded-host", "desk.tail1234.ts.net"),
                    (CSRF_HEADER, "1"),
                    ("origin", "https://desk.tail1234.ts.net")
                ]),
                None
            ),
            Ok(())
        );
    }
}
