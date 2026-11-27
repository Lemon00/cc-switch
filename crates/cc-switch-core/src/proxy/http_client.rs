//! 全局 HTTP 客户端模块
//!
//! 提供支持全局代理配置的 HTTP 客户端。
//! 所有需要发送 HTTP 请求的模块都应使用此模块提供的客户端。

use reqwest::Client;
use std::env;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::Duration;

/// 全局 HTTP 客户端实例
static CLIENT_CONFIG_LOCK: Mutex<()> = Mutex::new(());

static GLOBAL_CLIENT: OnceLock<RwLock<Client>> = OnceLock::new();

/// 不跟随重定向的全局客户端（与 GLOBAL_CLIENT 共享代理配置，避免高频重建连接池）
static NO_REDIRECT_CLIENT: OnceLock<RwLock<Client>> = OnceLock::new();

/// 仅允许 HTTPS 的全局客户端（与 GLOBAL_CLIENT 共享代理配置）
static HTTPS_ONLY_CLIENT: OnceLock<RwLock<Client>> = OnceLock::new();

/// 不走任何代理的客户端：上游是本机地址（Ollama、LM Studio 等）时使用
static DIRECT_CLIENT: OnceLock<Client> = OnceLock::new();

/// 不走代理且不跟随重定向的本机直连客户端
static DIRECT_NO_REDIRECT_CLIENT: OnceLock<Client> = OnceLock::new();

/// 当前代理 URL（用于日志和状态查询）
static CURRENT_PROXY_URL: OnceLock<RwLock<Option<String>>> = OnceLock::new();

/// CC Switch 代理服务器当前监听的端口
static CC_SWITCH_PROXY_PORT: OnceLock<RwLock<u16>> = OnceLock::new();

/// 设置 CC Switch 代理服务器的监听端口
///
/// 应在代理服务器启动时调用，以便系统代理检测能正确识别自己的端口
pub fn set_proxy_port(port: u16) {
    if let Some(lock) = CC_SWITCH_PROXY_PORT.get() {
        if let Ok(mut current_port) = lock.write() {
            *current_port = port;
            log::debug!("[GlobalProxy] Updated CC Switch proxy port to {port}");
        }
    } else {
        let _ = CC_SWITCH_PROXY_PORT.set(RwLock::new(port));
        log::debug!("[GlobalProxy] Initialized CC Switch proxy port to {port}");
    }
}

/// 获取 CC Switch 代理服务器的监听端口
fn get_proxy_port() -> u16 {
    CC_SWITCH_PROXY_PORT
        .get()
        .and_then(|lock| lock.read().ok())
        .map(|port| *port)
        .unwrap_or(15721) // 默认端口作为回退
}

/// 记录全局代理配置；客户端延迟到首次请求时创建
///
/// 应在应用启动时调用一次。
///
/// # Arguments
/// * `proxy_url` - 代理 URL，如 `http://127.0.0.1:7890` 或 `socks5://127.0.0.1:1080`
///   传入 None 或空字符串表示直连
pub fn init(proxy_url: Option<&str>) -> Result<(), String> {
    apply_proxy(proxy_url)
}

/// 验证代理配置（不应用）
///
/// 只验证代理 URL 是否有效，不实际更新全局客户端。
/// 用于在持久化之前验证配置的有效性。
///
/// # Arguments
/// * `proxy_url` - 代理 URL，None 或空字符串表示直连
///
/// # Returns
/// 验证成功返回 Ok(())，失败返回错误信息
pub fn validate_proxy(proxy_url: Option<&str>) -> Result<(), String> {
    let effective_url = proxy_url.filter(|s| !s.trim().is_empty());
    // 仅解析配置，不初始化 TLS / DNS 等运行时资源。
    if let Some(url) = effective_url {
        parse_proxy(url)?;
    }
    Ok(())
}

fn parse_proxy(url: &str) -> Result<reqwest::Proxy, String> {
    // 先验证 URL 格式和 scheme
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("Invalid proxy URL '{}': {}", mask_url(url), e))?;

    let scheme = parsed.scheme();
    if !["http", "https", "socks5", "socks5h"].contains(&scheme) {
        return Err(format!(
            "Invalid proxy scheme '{}' in URL '{}'. Supported: http, https, socks5, socks5h",
            scheme,
            mask_url(url)
        ));
    }

    let proxy = reqwest::Proxy::all(url)
        .map_err(|e| format!("Invalid proxy URL '{}': {}", mask_url(url), e))?;
    Ok(proxy)
}

/// 应用代理配置（假设已验证）
///
/// 直接应用代理配置到全局客户端，不做额外验证。
/// 应在 validate_proxy 成功后调用。
///
/// # Arguments
/// * `proxy_url` - 代理 URL，None 或空字符串表示直连
pub fn apply_proxy(proxy_url: Option<&str>) -> Result<(), String> {
    let _guard = CLIENT_CONFIG_LOCK
        .lock()
        .map_err(|_| "HTTP configuration lock poisoned")?;
    let effective_url = proxy_url.map(str::trim).filter(|s| !s.is_empty());
    validate_proxy(effective_url)?;
    // Build only pools that have actually been used. Serialize updates with first use
    // so a concurrently initialized pool cannot retain the previous proxy.
    let new_global = GLOBAL_CLIENT
        .get()
        .map(|_| build_client(effective_url))
        .transpose()?;
    let new_no_redirect = NO_REDIRECT_CLIENT
        .get()
        .map(|_| build_no_redirect_client(effective_url))
        .transpose()?;
    let new_https_only = HTTPS_ONLY_CLIENT
        .get()
        .map(|_| build_https_only_client(effective_url))
        .transpose()?;
    if let Some(client) = new_global {
        set_or_replace_client(&GLOBAL_CLIENT, client)?;
    }
    if let Some(client) = new_no_redirect {
        set_or_replace_client(&NO_REDIRECT_CLIENT, client)?;
    }
    if let Some(client) = new_https_only {
        set_or_replace_client(&HTTPS_ONLY_CLIENT, client)?;
    }
    *CURRENT_PROXY_URL
        .get_or_init(|| RwLock::new(None))
        .write()
        .map_err(|_| "HTTP proxy URL lock poisoned")? = effective_url.map(str::to_owned);
    log::info!(
        "[GlobalProxy] Configured: {}",
        effective_url
            .map(mask_url)
            .unwrap_or_else(|| "direct connection".into())
    );
    Ok(())
}

/// Lazily initialize the shared pool on the first outbound request.
pub fn get() -> Client {
    let _guard = CLIENT_CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    GLOBAL_CLIENT
        .get_or_init(|| {
            RwLock::new(
                build_client(get_current_proxy_url().as_deref()).unwrap_or_else(|error| {
                    log::warn!("[GlobalProxy] Failed to build client: {error}");
                    Client::new()
                }),
            )
        })
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// 按目标地址取客户端：本机地址直连，其余同 [`get`]
///
/// reqwest 不会自动绕开本机地址：配了全局代理时 `Proxy::all` 对所有目标生效，
/// 跟随系统代理时也只认 `NO_PROXY` 环境变量（macOS 不读「不使用代理的主机」）。
/// 本机推理服务因此会被送进代理，代理不能代连本机时请求就失败。
pub fn get_for_url(url: &str) -> Client {
    if is_loopback_url(url) {
        direct_client()
    } else {
        get()
    }
}

/// 发往 `url` 时应使用的上游代理：本机地址返回 None（直连），其余返回当前全局代理
pub fn proxy_url_for(url: &str) -> Option<String> {
    upstream_proxy_for(url, get_current_proxy_url())
}

fn upstream_proxy_for(url: &str, current: Option<String>) -> Option<String> {
    if is_loopback_url(url) {
        None
    } else {
        current
    }
}

fn direct_client() -> Client {
    DIRECT_CLIENT
        .get_or_init(|| {
            base_builder().no_proxy().build().unwrap_or_else(|e| {
                log::warn!("[GlobalProxy] Failed to build direct client: {e}");
                Client::new()
            })
        })
        .clone()
}

/// 地址的主机是不是本机：`localhost`、`*.localhost`、`127.0.0.0/8`、`::1`
pub fn is_loopback_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    match parsed.host() {
        Some(url::Host::Domain(domain)) => host_is_loopback(domain),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        None => false,
    }
}

fn host_is_loopback(host: &str) -> bool {
    let host = host.trim_end_matches('.');
    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost") {
        return true;
    }
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// 获取当前代理 URL
///
/// 返回当前配置的代理 URL，None 表示直连。
pub fn get_current_proxy_url() -> Option<String> {
    CURRENT_PROXY_URL
        .get()
        .and_then(|lock| lock.read().ok())
        .and_then(|url| url.clone())
}

/// 流式读取响应体，同时检查 `Content-Length` 与累计字节，避免 `.bytes()`/`.text()` 无界灌内存。
pub async fn read_body_with_limit(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if let Some(len) = response.content_length() {
        if len > limit as u64 {
            return Err(format!(
                "response Content-Length {len} exceeds limit of {limit} bytes"
            ));
        }
    }
    let mut buf = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Failed to read response body: {}", error.without_url()))?
    {
        if buf.len().saturating_add(chunk.len()) > limit {
            return Err(format!(
                "response body exceeds limit of {limit} bytes (read {}+{} so far)",
                buf.len(),
                chunk.len()
            ));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// 两种客户端共用的参数
fn base_builder() -> reqwest::ClientBuilder {
    let builder = Client::builder()
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .tcp_keepalive(Duration::from_secs(60))
        // 禁用 reqwest 自动解压：防止 reqwest 覆盖客户端原始 accept-encoding header。
        // 响应解压由 response_processor 根据 content-encoding 手动处理。
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd();

    // 部分网关（实测 AMD 的 Azure Application Gateway）会拒绝 rustls 的 HTTP/2
    // 请求并返回 400 HTML，而同一请求走 HTTP/1.1 正常。CLI 启动时设置
    // CC_SWITCH_PROXY_HTTP1=1 规避；GUI 不设置，继续协商 h2，行为不变。
    if std::env::var_os("CC_SWITCH_PROXY_HTTP1").is_some() {
        builder.http1_only()
    } else {
        builder
    }
}

fn set_or_replace_client(slot: &OnceLock<RwLock<Client>>, client: Client) -> Result<(), String> {
    if let Some(lock) = slot.get() {
        let mut guard = lock
            .write()
            .map_err(|_| "Failed to update HTTP client: lock poisoned".to_string())?;
        *guard = client;
    } else if let Err(existing) = slot.set(RwLock::new(client)) {
        // 并发初始化竞争：改写已存在的槽位
        if let Some(lock) = slot.get() {
            let mut guard = lock
                .write()
                .map_err(|_| "Failed to update HTTP client: lock poisoned".to_string())?;
            *guard = existing
                .into_inner()
                .map_err(|_| "Failed to update HTTP client: lock poisoned".to_string())?;
        }
    }
    Ok(())
}

fn build_no_redirect_client(proxy_url: Option<&str>) -> Result<Client, String> {
    build_client_from(
        base_builder().redirect(reqwest::redirect::Policy::none()),
        proxy_url,
    )
}

fn build_https_only_client(proxy_url: Option<&str>) -> Result<Client, String> {
    build_client_from(base_builder().https_only(true), proxy_url)
}

fn direct_no_redirect_client() -> Result<Client, String> {
    if let Some(client) = DIRECT_NO_REDIRECT_CLIENT.get() {
        return Ok(client.clone());
    }
    let client = base_builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let _ = DIRECT_NO_REDIRECT_CLIENT.set(client.clone());
    Ok(DIRECT_NO_REDIRECT_CLIENT.get().cloned().unwrap_or(client))
}

/// 不跟随重定向的客户端，代理设置与全局客户端一致。带自定义凭据请求头（如 `x-dsh-auth-token`）
/// 的请求用它：reqwest 跨主机重定向时只剥掉标准认证头，自定义头会被带到新主机。
pub fn no_redirect() -> Result<Client, String> {
    let _guard = CLIENT_CONFIG_LOCK
        .lock()
        .map_err(|_| "HTTP configuration lock poisoned")?;
    if let Some(lock) = NO_REDIRECT_CLIENT.get() {
        if let Ok(client) = lock.read() {
            return Ok(client.clone());
        }
    }
    let client = build_no_redirect_client(get_current_proxy_url().as_deref())?;
    let _ = NO_REDIRECT_CLIENT.set(RwLock::new(client.clone()));
    Ok(NO_REDIRECT_CLIENT
        .get()
        .and_then(|lock| lock.read().ok())
        .map(|c| c.clone())
        .unwrap_or(client))
}

/// 只走 https 的客户端（跳转到 http 直接报错），代理设置与全局客户端一致。
/// 带 `Authorization` 的请求用它：reqwest 只在换主机或换端口时去掉认证头，同主机
/// 同端口从 https 跳到 http（非默认端口时会发生）会把凭据明文发出去。
pub fn https_only() -> Result<Client, String> {
    let _guard = CLIENT_CONFIG_LOCK
        .lock()
        .map_err(|_| "HTTP configuration lock poisoned")?;
    if let Some(lock) = HTTPS_ONLY_CLIENT.get() {
        if let Ok(client) = lock.read() {
            return Ok(client.clone());
        }
    }
    let client = build_https_only_client(get_current_proxy_url().as_deref())?;
    let _ = HTTPS_ONLY_CLIENT.set(RwLock::new(client.clone()));
    Ok(HTTPS_ONLY_CLIENT
        .get()
        .and_then(|lock| lock.read().ok())
        .map(|c| c.clone())
        .unwrap_or(client))
}

/// A one-off no-redirect client for an explicit proxy (`None` follows the system
/// proxy), leaving the saved global proxy untouched.
pub fn no_redirect_with_proxy(
    proxy_url: Option<&str>,
    timeout: Duration,
) -> Result<Client, String> {
    build_client_from(
        base_builder()
            .timeout(timeout)
            .connect_timeout(timeout.min(Duration::from_secs(5)))
            .redirect(reqwest::redirect::Policy::none()),
        proxy_url,
    )
}

/// 不跟随重定向、按目标地址选代理的客户端：本机地址直连，其余同 [`no_redirect`]。
pub fn no_redirect_for_url(url: &str) -> Result<Client, String> {
    if is_loopback_url(url) {
        direct_no_redirect_client()
    } else {
        no_redirect()
    }
}

/// 构建 HTTP 客户端
fn build_client(proxy_url: Option<&str>) -> Result<Client, String> {
    build_client_from(base_builder(), proxy_url)
}

fn build_client_from(
    mut builder: reqwest::ClientBuilder,
    proxy_url: Option<&str>,
) -> Result<Client, String> {
    // 有代理地址则使用代理，否则跟随系统代理
    if let Some(url) = proxy_url {
        let proxy = parse_proxy(url)?;
        builder = builder.proxy(proxy);
        log::debug!("[GlobalProxy] Proxy configured: {}", mask_url(url));
    } else {
        // 未设置全局代理时，让 reqwest 自动检测系统代理（环境变量）
        // 若系统代理指向本机，禁用系统代理避免自环
        if system_proxy_points_to_loopback() {
            builder = builder.no_proxy();
            log::warn!(
                "[GlobalProxy] System proxy points to localhost, bypassing to avoid recursion"
            );
        } else {
            log::debug!("[GlobalProxy] Following system proxy (no explicit proxy configured)");
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

fn system_proxy_points_to_loopback() -> bool {
    const KEYS: [&str; 6] = [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ];

    KEYS.iter()
        .filter_map(|key| env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .any(|value| proxy_points_to_loopback(&value))
}

fn proxy_points_to_loopback(value: &str) -> bool {
    // 检查是否指向 CC Switch 自己的代理端口
    // 只有指向自己的代理才需要跳过，避免递归
    fn is_cc_switch_proxy_port(port: Option<u16>) -> bool {
        let cc_switch_port = get_proxy_port();
        port == Some(cc_switch_port)
    }

    if let Ok(parsed) = url::Url::parse(value) {
        if let Some(host) = parsed.host_str() {
            // 只有当主机是 loopback 且端口是 CC Switch 的端口时才返回 true
            return host_is_loopback(host) && is_cc_switch_proxy_port(parsed.port());
        }
        return false;
    }

    let with_scheme = format!("http://{value}");
    if let Ok(parsed) = url::Url::parse(&with_scheme) {
        if let Some(host) = parsed.host_str() {
            return host_is_loopback(host) && is_cc_switch_proxy_port(parsed.port());
        }
    }

    false
}

/// 隐藏 URL 中的敏感信息（用于日志）
pub fn mask_url(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        // 隐藏用户名和密码，保留 scheme、host 和端口
        let host = parsed.host_str().unwrap_or("?");
        match parsed.port() {
            Some(port) => format!("{}://{}:{}", parsed.scheme(), host, port),
            None => format!("{}://{}", parsed.scheme(), host),
        }
    } else {
        // URL 解析失败，返回部分内容。截断点回退到最近的字符边界，
        // 避免在多字节 UTF-8 字符中间切割导致 panic。
        if url.len() > 20 {
            let cut = (0..=20)
                .rev()
                .find(|&i| url.is_char_boundary(i))
                .unwrap_or(0);
            format!("{}...", &url[..cut])
        } else {
            url.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    #[tokio::test]
    async fn bounded_body_reader_handles_exact_limit_and_rejects_extra_bytes() {
        for (body, limit, succeeds) in [("", 0, true), ("abcd", 4, true), ("abcde", 4, false)] {
            let response: reqwest::Response = http::Response::builder()
                .body(body.to_owned())
                .unwrap()
                .into();
            let result = read_body_with_limit(response, limit).await;
            assert_eq!(result.is_ok(), succeeds);
            if succeeds {
                assert_eq!(result.unwrap(), body.as_bytes());
            }
        }
    }

    #[tokio::test]
    async fn bounded_body_reader_rejects_chunked_overflow_without_content_length() {
        let chunks = futures::stream::iter([
            Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"abc")),
            Ok(bytes::Bytes::from_static(b"de")),
        ]);
        let response: reqwest::Response = http::Response::builder()
            .body(reqwest::Body::wrap_stream(chunks))
            .unwrap()
            .into();
        assert_eq!(response.content_length(), None);
        assert!(read_body_with_limit(response, 4).await.is_err());
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_mask_url() {
        assert_eq!(mask_url("http://127.0.0.1:7890"), "http://127.0.0.1:7890");
        assert_eq!(
            mask_url("http://user:pass@127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            mask_url("socks5://admin:secret@proxy.example.com:1080"),
            "socks5://proxy.example.com:1080"
        );
        // 无端口的 URL 不应显示 ":?"
        assert_eq!(
            mask_url("http://proxy.example.com"),
            "http://proxy.example.com"
        );
        assert_eq!(
            mask_url("https://user:pass@proxy.example.com"),
            "https://proxy.example.com"
        );
    }

    #[test]
    fn test_mask_url_does_not_panic_on_multibyte_boundary() {
        // 一个无法被 Url::parse 解析、且在字节 20 处正好切在多字节字符中间的字符串。
        // 回归 https://github.com/farion1231/cc-switch 的 mask_url 越界 panic。
        let bad = "这是一个无效的代理地址不能解析";
        assert!(bad.len() > 20 && !bad.is_char_boundary(20));
        let masked = mask_url(bad);
        assert!(masked.ends_with("..."));
    }

    #[test]
    fn test_is_loopback_url() {
        for url in [
            "http://localhost:11434",
            "http://LOCALHOST",
            "http://localhost./v1",
            "http://127.0.0.1:1234/v1",
            "http://127.1.2.3",
            "http://[::1]:8000",
            "http://[::ffff:127.0.0.1]:8000",
            "http://foo.localhost",
            "https://Api.LocalHost:8443/v1",
        ] {
            assert!(is_loopback_url(url), "{url} should be loopback");
        }
        for url in [
            "http://192.168.1.10:11434",
            "http://10.0.0.2",
            "https://example.com",
            "https://localhost.evil.com",
            "https://evillocalhost",
            "http://[::2]",
            "localhost:11434",
            "",
        ] {
            assert!(!is_loopback_url(url), "{url} should not be loopback");
        }
    }

    #[test]
    fn test_upstream_proxy_for_skips_loopback() {
        let current = Some("http://127.0.0.1:7890".to_string());
        assert_eq!(
            upstream_proxy_for("http://localhost:11434/v1/messages", current.clone()),
            None
        );
        assert_eq!(
            upstream_proxy_for("http://127.0.0.1:1234/v1/chat/completions", current.clone()),
            None
        );
        assert_eq!(
            upstream_proxy_for("https://api.example.com/v1/messages", current.clone()),
            current
        );
        assert_eq!(
            upstream_proxy_for("https://api.example.com/v1/messages", None),
            None
        );
    }

    /// 起一个只回 200 的一次性本机 HTTP 服务，返回它的地址
    fn spawn_ok_server() -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(4) {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
            }
        });
        format!("http://{addr}/v1/models")
    }

    #[tokio::test]
    async fn test_loopback_target_bypasses_unreachable_proxy() {
        let url = spawn_ok_server();
        // 代理指向一个连不上的端口：经代理的客户端连本机服务也会失败（缺口本身）
        let proxied = build_client(Some("http://127.0.0.1:1")).unwrap();
        assert!(proxied
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .is_err());
        // 本机地址选到的直连客户端不受代理影响
        assert!(is_loopback_url(&url));
        let resp = direct_client()
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .expect("direct client should reach the local server");
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[test]
    fn test_build_client_direct() {
        let result = build_client(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_client_with_http_proxy() {
        let result = build_client(Some("http://127.0.0.1:7890"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_client_with_socks5_proxy() {
        let result = build_client(Some("socks5://127.0.0.1:1080"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_client_invalid_url() {
        // reqwest::Proxy::all 对某些无效 URL 不会立即报错
        // 使用明确无效的 scheme 来触发错误
        let result = build_client(Some("invalid-scheme://127.0.0.1:7890"));
        assert!(result.is_err(), "Should reject invalid proxy scheme");
    }

    #[test]
    #[serial_test::serial]
    fn test_proxy_points_to_loopback() {
        // 设置 CC Switch 代理端口为 15721（默认值）
        set_proxy_port(15721);

        // 只有指向 CC Switch 自己端口的 loopback 地址才返回 true
        assert!(proxy_points_to_loopback("http://127.0.0.1:15721"));
        assert!(proxy_points_to_loopback("socks5://localhost:15721"));
        assert!(proxy_points_to_loopback("127.0.0.1:15721"));

        // 其他 loopback 端口不应该被跳过（允许使用其他本地代理工具）
        assert!(!proxy_points_to_loopback("http://127.0.0.1:7890"));
        assert!(!proxy_points_to_loopback("socks5://localhost:1080"));

        // 非 loopback 地址不应该被跳过
        assert!(!proxy_points_to_loopback("http://192.168.1.10:7890"));
        assert!(!proxy_points_to_loopback("http://192.168.1.10:15721"));
    }

    #[test]
    #[serial_test::serial]
    fn test_system_proxy_points_to_loopback() {
        let _guard = env_lock().lock().unwrap();

        // 设置 CC Switch 代理端口
        set_proxy_port(15721);

        let keys = [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ];

        for key in &keys {
            std::env::remove_var(key);
        }

        // 指向 CC Switch 端口的代理应该被跳过
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:15721");
        assert!(system_proxy_points_to_loopback());

        // 指向其他端口的本地代理不应该被跳过
        std::env::set_var("HTTP_PROXY", "http://127.0.0.1:7890");
        assert!(!system_proxy_points_to_loopback());

        // 非 loopback 地址不应该被跳过
        std::env::set_var("HTTP_PROXY", "http://10.0.0.2:7890");
        assert!(!system_proxy_points_to_loopback());

        for key in &keys {
            std::env::remove_var(key);
        }
    }

    #[tokio::test]
    async fn test_no_redirect_for_url_reuses_keep_alive_connection() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let accepts = Arc::new(AtomicUsize::new(0));
        let accepts_srv = accepts.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                accepts_srv.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    while let Ok(n) = stream.read(&mut buf).await {
                        if n == 0 {
                            break;
                        }
                        let resp =
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nok";
                        if stream.write_all(resp).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });

        let url = format!("http://{addr}/ping");
        for _ in 0..10 {
            let body = no_redirect_for_url(&url)
                .unwrap()
                .get(&url)
                .send()
                .await
                .unwrap()
                .bytes()
                .await
                .unwrap();
            assert_eq!(body.as_ref(), b"ok");
        }
        server.abort();

        assert!(
            accepts.load(Ordering::SeqCst) <= 2,
            "cached no_redirect_for_url client should reuse keep-alive TCP connection"
        );
    }
    #[test]
    fn startup_proxy_validation_accepts_unreachable_but_valid_addresses() {
        for proxy in [
            None,
            Some(""),
            Some("  "),
            Some("http://127.0.0.1:1"),
            Some("socks5h://offline.invalid:1080"),
        ] {
            assert!(validate_proxy(proxy).is_ok(), "{proxy:?}");
        }
        for proxy in ["ftp://127.0.0.1:1", "not a URL", "http://"] {
            assert!(validate_proxy(Some(proxy)).is_err(), "{proxy}");
        }
    }
}
