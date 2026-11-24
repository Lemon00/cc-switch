//! Reject requests that a web page made, so an open browser tab cannot spend the
//! user's credentials through the local proxy.
//!
//! Remote peers must authenticate before using stored provider credentials.
//! CLI clients never send
//! `Origin`, while browsers attach it to every POST (including `no-cors` ones that
//! skip preflight) and to every WebSocket handshake, so rejecting it stops a page from
//! calling the proxy cross-site. A DNS-rebinding page is same-origin and can read
//! GET responses, but it has to reach us under its own domain name. Requests
//! trusted only because they are local must use a local or configured Host,
//! including loopback peers reaching a wildcard listener.
//!
//! Inbound credentials are stripped and request body reads have a deadline.

use super::server::ProxyState;
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use serde_json::json;
use std::net::IpAddr;
use subtle::ConstantTimeEq;

#[derive(Clone, Copy)]
pub(crate) struct PeerAddr(pub std::net::SocketAddr);

fn valid_lan_token(headers: &HeaderMap, expected: &str) -> bool {
    if expected.len() < 32 {
        return false;
    }
    [
        "x-cc-switch-token",
        "authorization",
        "x-api-key",
        "x-goog-api-key",
    ]
    .into_iter()
    .filter_map(|name| {
        let mut values = headers.get_all(name).iter();
        let value = values.next()?.to_str().ok()?;
        if values.next().is_some() {
            return None;
        }
        if name == "authorization" {
            let (scheme, token) = value.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("bearer")
                .then_some(token.trim())
        } else {
            Some(value.trim())
        }
    })
    .any(|token| bool::from(token.as_bytes().ct_eq(expected.as_bytes())))
}

fn timed_request_body(body: axum::body::Body, duration: std::time::Duration) -> axum::body::Body {
    let deadline = tokio::time::Instant::now() + duration;
    let mut stream = body.into_data_stream();
    axum::body::Body::from_stream(async_stream::stream! {
        loop {
            match tokio::time::timeout_at(deadline, stream.next()).await {
                Ok(Some(Ok(chunk))) => yield Ok::<_, std::io::Error>(chunk),
                Ok(Some(Err(error))) => {
                    yield Err(std::io::Error::other(error));
                    break;
                }
                Ok(None) => break,
                Err(_) => {
                    yield Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Request body timed out"));
                    break;
                }
            }
        }
    })
}

fn strip_lan_credentials(headers: &mut HeaderMap, expected: Option<&str>) {
    headers.remove("x-cc-switch-token");
    let Some(expected) = expected else { return };
    for name in ["authorization", "x-api-key", "x-goog-api-key"] {
        let carries_secret = headers.get_all(name).iter().any(|value| {
            let mut candidate = HeaderMap::new();
            candidate.insert(name, value.clone());
            valid_lan_token(&candidate, expected)
        });
        if carries_secret {
            headers.remove(name);
        }
    }
}

/// Claude Desktop's namespace already requires its gateway token, which a page
/// cannot know, so the desktop app is not held to the header rules.
const TOKEN_PROTECTED_PREFIX: &str = "/claude-desktop/";

fn lan_request_authorized(request: &Request, listen_address: &str, token: Option<&str>) -> bool {
    is_loopback_listener(listen_address)
        || request
            .extensions()
            .get::<PeerAddr>()
            .is_some_and(|peer| peer.0.ip().to_canonical().is_loopback())
        || request.uri().path().starts_with(TOKEN_PROTECTED_PREFIX)
        || token.is_some_and(|token| valid_lan_token(request.headers(), token))
}

fn local_peer_requires_host_check(request: &Request, token: Option<&str>) -> bool {
    request
        .extensions()
        .get::<PeerAddr>()
        .is_some_and(|peer| peer.0.ip().to_canonical().is_loopback())
        && !request.uri().path().starts_with(TOKEN_PROTECTED_PREFIX)
        && !token.is_some_and(|token| valid_lan_token(request.headers(), token))
}

pub(super) async fn reject_browser_requests(
    State(state): State<ProxyState>,
    request: Request,
    next: Next,
) -> Response {
    let listen_address = state.config.read().await.listen_address.clone();
    let lan_token = crate::settings::get_settings().proxy_lan_token;
    let client_host = crate::settings::get_proxy_client_host();
    if !lan_request_authorized(&request, &listen_address, lan_token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, Json(json!({
            "error": {"type": "authentication_error", "message": "A valid CC Switch LAN proxy token is required"}
        }))).into_response();
    }
    let checked = check_request(
        request.uri().path(),
        request.headers(),
        &listen_address,
        client_host.as_deref(),
    )
    .and_then(|()| {
        if local_peer_requires_host_check(&request, lan_token.as_deref()) {
            check_local_host(request.headers(), client_host.as_deref())
        } else {
            Ok(())
        }
    });
    match checked {
        Ok(()) => {
            let (mut parts, body) = request.into_parts();
            strip_lan_credentials(&mut parts.headers, lan_token.as_deref());
            let request = Request::from_parts(
                parts,
                timed_request_body(body, std::time::Duration::from_secs(60)),
            );
            next.run(request).await
        }
        Err(reason) => {
            log::warn!(
                "[ProxyServer] rejected {} {}: {reason}",
                request.method(),
                request.uri().path()
            );
            (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": {
                        "type": "forbidden",
                        "message": format!("CC Switch local proxy: {reason}"),
                    }
                })),
            )
                .into_response()
        }
    }
}

fn check_request(
    path: &str,
    headers: &HeaderMap,
    listen_address: &str,
    client_host: Option<&str>,
) -> Result<(), &'static str> {
    if !path.starts_with(TOKEN_PROTECTED_PREFIX) && headers.contains_key(header::ORIGIN) {
        return Err("requests from web pages are not accepted");
    }
    if is_loopback_listener(listen_address) {
        check_local_host(headers, client_host)?;
    }
    Ok(())
}

fn check_local_host(headers: &HeaderMap, client_host: Option<&str>) -> Result<(), &'static str> {
    if let Some(host) = headers.get(header::HOST) {
        let host = host.to_str().ok().and_then(host_name);
        if !host.is_some_and(|host| is_local_host(&host, client_host)) {
            return Err("unexpected Host header");
        }
    }
    Ok(())
}

/// Loopback listeners always require a local Host; wildcard listeners also check
/// peers that rely on the loopback authentication exemption.
fn is_loopback_listener(listen_address: &str) -> bool {
    is_loopback_listen_address(listen_address)
}

/// 供启动绑定与入口 guard 共用：判断监听地址是否仅本机可达。
pub(crate) fn is_loopback_listen_address(listen_address: &str) -> bool {
    let address = listen_address.trim().trim_matches(['[', ']']);
    address.eq_ignore_ascii_case("localhost")
        // `to_canonical`：`::ffff:127.0.0.1` 也绑在回环上，但 IPv6 的 is_loopback 不认它
        || address
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.to_canonical().is_loopback())
}

/// The host part of a Host header, lowercased, without port, brackets or trailing dot.
fn host_name(value: &str) -> Option<String> {
    let authority: http::uri::Authority = value.trim().parse().ok()?;
    let host = authority
        .host()
        .trim_matches(['[', ']'])
        .trim_end_matches('.');
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Names a rebinding page cannot use: IP literals (rebinding needs a domain),
/// `localhost` and the reserved private-use `.localhost` / `.local` / `.internal`
/// (the last covers `host.docker.internal`), plus the client address the user set.
fn is_local_host(host: &str, client_host: Option<&str>) -> bool {
    host.parse::<IpAddr>().is_ok()
        || host == "localhost"
        || [".localhost", ".local", ".internal"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
        || client_host.is_some_and(|client| {
            client
                .trim()
                .trim_matches(['[', ']'])
                .trim_end_matches('.')
                .eq_ignore_ascii_case(host)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_auth_uses_tcp_peer_not_forwarded_headers() {
        let secret = "0123456789abcdef0123456789abcdef";
        let mut request = Request::builder()
            .uri("/v1/messages")
            .header("x-forwarded-for", "127.0.0.1")
            .body(axum::body::Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(PeerAddr("192.168.1.2:1234".parse().unwrap()));
        assert!(!lan_request_authorized(&request, "0.0.0.0", Some(secret)));
        request
            .headers_mut()
            .insert("x-api-key", secret.parse().unwrap());
        assert!(lan_request_authorized(&request, "0.0.0.0", Some(secret)));
        assert!(!lan_request_authorized(
            &request,
            "0.0.0.0",
            Some("rotated-secret-0123456789abcdef0123456789")
        ));
        request.headers_mut().remove("x-api-key");
        request
            .extensions_mut()
            .insert(PeerAddr("[::ffff:127.0.0.1]:1234".parse().unwrap()));
        assert!(lan_request_authorized(&request, "::", Some(secret)));
        request.extensions_mut().remove::<PeerAddr>();
        assert!(!lan_request_authorized(&request, "0.0.0.0", Some(secret)));
        assert!(lan_request_authorized(&request, "127.0.0.1", Some(secret)));
    }

    #[tokio::test]
    async fn router_rejects_unauthenticated_remote_peers() {
        use tower::Service;
        let state = ProxyState::for_test(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        state.config.write().await.listen_address = "0.0.0.0".into();
        let mut router = axum::Router::new()
            .route("/health", axum::routing::get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                reject_browser_requests,
            ));
        let mut request = Request::builder()
            .uri("/health")
            .header("x-forwarded-for", "127.0.0.1")
            .body(axum::body::Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(PeerAddr("192.168.1.2:1234".parse().unwrap()));
        assert_eq!(
            router.call(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let mut request = Request::builder()
            .uri("/health")
            .body(axum::body::Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(PeerAddr("127.0.0.1:1234".parse().unwrap()));
        assert_eq!(router.call(request).await.unwrap().status(), StatusCode::OK);

        for listen_address in ["0.0.0.0", "::"] {
            let state = ProxyState::for_test(std::sync::Arc::new(
                crate::database::Database::memory().unwrap(),
            ));
            state.config.write().await.listen_address = listen_address.into();
            let mut router = axum::Router::new()
                .route("/status", axum::routing::get(|| async { "private status" }))
                .layer(axum::middleware::from_fn_with_state(
                    state,
                    reject_browser_requests,
                ));
            for (host, expected) in [
                ("rebind.example:15721", StatusCode::FORBIDDEN),
                ("localhost:15721", StatusCode::OK),
                ("[::1]:15721", StatusCode::OK),
            ] {
                let mut request = Request::builder()
                    .uri("/status")
                    .header(header::HOST, host)
                    .body(axum::body::Body::empty())
                    .unwrap();
                request
                    .extensions_mut()
                    .insert(PeerAddr("[::ffff:127.0.0.1]:1234".parse().unwrap()));
                assert_eq!(router.call(request).await.unwrap().status(), expected);
            }
        }
    }

    #[test]
    fn authenticated_local_clients_can_still_use_lan_hostnames() {
        let secret = "0123456789abcdef0123456789abcdef";
        let mut request = Request::builder()
            .uri("/v1/models")
            .header(header::HOST, "devbox.corp.example")
            .header("x-cc-switch-token", secret)
            .body(axum::body::Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(PeerAddr("127.0.0.1:1234".parse().unwrap()));
        assert!(!local_peer_requires_host_check(&request, Some(secret)));
        request.headers_mut().remove("x-cc-switch-token");
        assert!(local_peer_requires_host_check(&request, Some(secret)));
    }

    #[test]
    fn lan_tokens_require_exact_secret_and_accept_supported_clients() {
        let secret = "0123456789abcdef0123456789abcdef";
        for name in [
            "x-cc-switch-token",
            "x-api-key",
            "x-goog-api-key",
            "authorization",
        ] {
            let value = if name == "authorization" {
                format!("Bearer {secret}")
            } else {
                secret.to_string()
            };
            assert!(valid_lan_token(&headers(&[(name, &value)]), secret));
            assert!(!valid_lan_token(
                &headers(&[(name, "PROXY_MANAGED")]),
                secret
            ));
        }
        assert!(!valid_lan_token(&HeaderMap::new(), secret));
        assert!(!valid_lan_token(
            &headers(&[("x-forwarded-for", "127.0.0.1")]),
            secret
        ));
        assert!(!valid_lan_token(&headers(&[("x-api-key", secret)]), ""));
        let mut duplicate = headers(&[("x-api-key", secret)]);
        duplicate.append("x-api-key", "wrong".parse().unwrap());
        assert!(!valid_lan_token(&duplicate, secret));
    }

    #[test]
    fn inbound_secrets_are_removed_without_losing_upstream_credentials() {
        let secret = "0123456789abcdef0123456789abcdef";
        let mut incoming = headers(&[
            ("x-cc-switch-token", secret),
            ("authorization", "Bearer official-login"),
            ("x-api-key", secret),
        ]);
        strip_lan_credentials(&mut incoming, Some(secret));
        assert!(!incoming.contains_key("x-cc-switch-token"));
        assert!(!incoming.contains_key("x-api-key"));
        assert_eq!(incoming["authorization"], "Bearer official-login");
        incoming.insert("authorization", format!("Bearer {secret}").parse().unwrap());
        incoming.insert("x-goog-api-key", secret.parse().unwrap());
        strip_lan_credentials(&mut incoming, Some(secret));
        assert!(!incoming.contains_key("authorization"));
        assert!(!incoming.contains_key("x-goog-api-key"));
    }

    #[tokio::test]
    async fn stalled_request_body_expires_without_timing_out_responses() {
        let body = axum::body::Body::from_stream(futures::stream::pending::<
            Result<bytes::Bytes, std::io::Error>,
        >());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            axum::body::to_bytes(
                timed_request_body(body, std::time::Duration::from_millis(5)),
                1024,
            ),
        )
        .await;
        assert!(result.unwrap().is_err());
    }

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, value.parse().unwrap());
        }
        map
    }

    #[test]
    fn cli_requests_pass() {
        for host in ["127.0.0.1:15721", "localhost:15721", "[::1]:15721"] {
            let h = headers(&[("host", host)]);
            assert_eq!(check_request("/v1/messages", &h, "127.0.0.1", None), Ok(()));
        }
        // No Host at all (HTTP/1.0 clients) is not a browser.
        assert_eq!(
            check_request("/v1/messages", &HeaderMap::new(), "127.0.0.1", None),
            Ok(())
        );
    }

    #[test]
    fn any_origin_is_rejected_outside_claude_desktop() {
        for origin in ["https://evil.example", "null", "tauri://localhost"] {
            let h = headers(&[("host", "127.0.0.1:15721"), ("origin", origin)]);
            for path in ["/v1/messages", "/v1/responses", "/v1beta/models", "/status"] {
                assert!(check_request(path, &h, "127.0.0.1", None).is_err());
            }
            // LAN listeners are covered too: the Origin rule does not depend on Host.
            assert!(check_request("/v1/messages", &h, "0.0.0.0", None).is_err());
        }
    }

    #[test]
    fn claude_desktop_keeps_its_token_check_only() {
        let h = headers(&[("host", "127.0.0.1:15721"), ("origin", "app://claude")]);
        assert_eq!(
            check_request("/claude-desktop/v1/messages", &h, "127.0.0.1", None),
            Ok(())
        );
        // Rebinding still applies there: the Host rule is not exempted.
        let h = headers(&[("host", "rebind.evil.example:15721")]);
        assert!(check_request("/claude-desktop/v1/models", &h, "127.0.0.1", None).is_err());
    }

    #[test]
    fn rebinding_hosts_are_rejected_on_loopback_listener() {
        for host in [
            "rebind.evil.example:15721",
            "evil.example.",
            "localhost.evil.example",
        ] {
            let h = headers(&[("host", host)]);
            assert!(
                check_request("/status", &h, "127.0.0.1", None).is_err(),
                "{host}"
            );
            assert!(check_request("/status", &h, "::1", None).is_err(), "{host}");
            assert!(
                check_request("/status", &h, "[::ffff:127.0.0.1]", None).is_err(),
                "{host}"
            );
        }
    }

    #[test]
    fn local_names_and_configured_client_host_pass() {
        for host in [
            "host.docker.internal:15721",
            "my-mac.local:15721",
            "app.localhost:15721",
            "LOCALHOST.:15721",
            "172.25.144.1:15721",
        ] {
            let h = headers(&[("host", host)]);
            assert_eq!(
                check_request("/v1/messages", &h, "127.0.0.1", None),
                Ok(()),
                "{host}"
            );
        }
        let h = headers(&[("host", "Devbox.Example.com:15721")]);
        assert_eq!(
            check_request(
                "/v1/messages",
                &h,
                "127.0.0.1",
                Some(" devbox.example.com ")
            ),
            Ok(())
        );
    }

    #[test]
    fn lan_listener_accepts_any_host_name() {
        let h = headers(&[("host", "devbox.corp.example:15721")]);
        assert_eq!(check_request("/v1/messages", &h, "0.0.0.0", None), Ok(()));
        assert_eq!(
            check_request("/v1/messages", &h, "192.168.1.5", None),
            Ok(())
        );
    }
}
