//! 全局出站代理相关命令
//!
//! 提供获取、设置和测试全局代理的 Tauri 命令。

use super::run_db_task;
use crate::proxy::http_client;
use crate::store::AppState;
use serde::Serialize;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// The HTTP clients are process-wide, so their persisted configuration and
// runtime update must share one lock across all command invocations.
static PROXY_CONFIG_UPDATE: Mutex<()> = Mutex::new(());

/// Rebuild outbound clients from the restored database under the same lock as
/// interactive proxy saves. Do not rewrite the restored value on failure.
pub(crate) fn reload_proxy_from_database(db: &crate::database::Database) -> Result<(), String> {
    let _guard = PROXY_CONFIG_UPDATE
        .lock()
        .map_err(|_| "Global proxy configuration lock poisoned".to_string())?;
    let url = db
        .get_global_proxy_url()
        .map_err(|error| error.to_string())?;
    http_client::validate_proxy(url.as_deref())?;
    http_client::apply_proxy(url.as_deref())
}

fn save_and_apply_proxy(
    db: &crate::database::Database,
    url: Option<&str>,
    apply: impl FnOnce(Option<&str>) -> Result<(), String>,
) -> Result<(), String> {
    http_client::validate_proxy(url)?;
    let _guard = PROXY_CONFIG_UPDATE
        .lock()
        .map_err(|_| "Global proxy configuration lock poisoned".to_string())?;
    let previous = db.get_global_proxy_url().map_err(|e| e.to_string())?;
    db.set_global_proxy_url(url).map_err(|e| e.to_string())?;
    if let Err(error) = apply(url) {
        return match db.set_global_proxy_url(previous.as_deref()) {
            Ok(()) => Err(error),
            Err(restore) => Err(format!(
                "{error}; failed to restore proxy configuration: {restore}"
            )),
        };
    }
    Ok(())
}

/// 获取全局代理 URL
///
/// 返回当前配置的代理 URL，null 表示直连。
#[tauri::command]
pub async fn get_global_proxy_url(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    let result = run_db_task(&state, |state| {
        state.db.get_global_proxy_url().map_err(|e| e.to_string())
    })
    .await?;
    log::debug!(
        "[GlobalProxy] [GP-010] Read from database: {}",
        result
            .as_ref()
            .map(|u| http_client::mask_url(u))
            .unwrap_or_else(|| "None".to_string())
    );
    Ok(result)
}

/// 设置全局代理 URL
///
/// - 传入非空字符串：启用代理
/// - 传入空字符串：清除代理（直连）
///
/// 执行顺序：先验证 → 写 DB → 再应用
/// 这样确保 DB 写失败时不会出现运行态与持久化不一致的问题
#[tauri::command]
pub async fn set_global_proxy_url(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<(), String> {
    // 调试：显示接收到的 URL 信息（不包含敏感内容）
    let has_auth = url.contains('@') && (url.starts_with("http://") || url.starts_with("socks"));
    log::debug!(
        "[GlobalProxy] [GP-011] Received URL: length={}, has_auth={}",
        url.len(),
        has_auth
    );

    let url_opt = if url.trim().is_empty() {
        None
    } else {
        Some(url.trim().to_string())
    };

    // Both steps run in the same blocking task: cancellation cannot stop after
    // persistence but before the runtime update, and concurrent saves serialize.
    run_db_task(&state, move |state| {
        save_and_apply_proxy(&state.db, url_opt.as_deref(), http_client::apply_proxy)?;
        log::info!(
            "[GlobalProxy] [GP-009] Configuration updated: {}",
            url_opt
                .as_deref()
                .map(http_client::mask_url)
                .unwrap_or_else(|| "direct connection".to_string())
        );
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use std::sync::{mpsc, Arc};

    #[test]
    fn failed_runtime_update_restores_the_persisted_proxy() {
        let db = Database::memory().unwrap();
        db.set_global_proxy_url(Some("http://old.example:8080"))
            .unwrap();
        let error = save_and_apply_proxy(&db, Some("http://new.example:8080"), |_| {
            Err("client construction failed".to_string())
        })
        .unwrap_err();
        assert_eq!(error, "client construction failed");
        assert_eq!(
            db.get_global_proxy_url().unwrap().as_deref(),
            Some("http://old.example:8080")
        );
    }

    #[test]
    fn invalid_proxy_is_neither_persisted_nor_applied() {
        let db = Database::memory().unwrap();
        assert!(
            save_and_apply_proxy(&db, Some("ftp://invalid.example"), |_| {
                panic!("invalid configuration must never be applied")
            })
            .is_err()
        );
        assert_eq!(db.get_global_proxy_url().unwrap(), None);
    }

    #[test]
    fn concurrent_saves_keep_runtime_and_database_in_the_same_order() {
        let db = Arc::new(Database::memory().unwrap());
        let runtime = Arc::new(Mutex::new(None));
        let (applying_tx, applying_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let db_first = db.clone();
            let runtime_first = runtime.clone();
            let first = scope.spawn(move || {
                save_and_apply_proxy(&db_first, Some("http://first.example:8080"), |url| {
                    applying_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    *runtime_first.lock().unwrap() = url.map(str::to_owned);
                    Ok(())
                })
            });
            applying_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let db_second = db.clone();
            let runtime_second = runtime.clone();
            let second = scope.spawn(move || {
                started_tx.send(()).unwrap();
                let result =
                    save_and_apply_proxy(&db_second, Some("http://second.example:8080"), |url| {
                        *runtime_second.lock().unwrap() = url.map(str::to_owned);
                        Ok(())
                    });
                finished_tx.send(()).unwrap();
                result
            });
            started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let second_finished = finished_rx.recv_timeout(Duration::from_millis(100));
            let persisted_while_applying = db.get_global_proxy_url().unwrap();
            release_tx.send(()).unwrap();
            first.join().unwrap().unwrap();
            second.join().unwrap().unwrap();
            assert!(matches!(
                second_finished,
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            assert_eq!(
                persisted_while_applying.as_deref(),
                Some("http://first.example:8080")
            );
        });
        assert_eq!(db.get_global_proxy_url().unwrap(), *runtime.lock().unwrap());
        assert_eq!(
            runtime.lock().unwrap().as_deref(),
            Some("http://second.example:8080")
        );
    }
}

/// 代理测试结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyTestResult {
    /// 是否连接成功
    pub success: bool,
    /// 延迟（毫秒）
    pub latency_ms: u64,
    /// 错误信息
    pub error: Option<String>,
}

/// 测试代理连接
///
/// 通过指定的代理 URL 发送测试请求，返回连接结果和延迟。
/// 使用多个测试目标，任一成功即认为代理可用。
#[tauri::command]
pub async fn test_proxy_url(url: String) -> Result<ProxyTestResult, String> {
    if url.trim().is_empty() {
        return Err("Proxy URL is empty".to_string());
    }

    let start = Instant::now();

    // 构建带代理的临时客户端
    let proxy = reqwest::Proxy::all(&url).map_err(|e| format!("Invalid proxy URL: {e}"))?;

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build client: {e}"))?;

    // 使用多个测试目标，提高兼容性
    // 优先使用 httpbin（专门用于 HTTP 测试），回退到其他公共端点
    let test_urls = [
        "https://httpbin.org/get",
        "https://www.google.com",
        "https://api.anthropic.com",
    ];

    let mut last_error = None;

    for test_url in test_urls {
        match client.head(test_url).send().await {
            Ok(resp) => {
                let latency = start.elapsed().as_millis() as u64;
                log::debug!(
                    "[GlobalProxy] Test successful: {} -> {} via {} ({}ms)",
                    http_client::mask_url(&url),
                    test_url,
                    resp.status(),
                    latency
                );
                return Ok(ProxyTestResult {
                    success: true,
                    latency_ms: latency,
                    error: None,
                });
            }
            Err(e) => {
                log::debug!("[GlobalProxy] Test to {test_url} failed: {e}");
                last_error = Some(e);
            }
        }
    }

    // 所有测试目标都失败
    let latency = start.elapsed().as_millis() as u64;
    let error_msg = last_error
        .map(|e| e.to_string())
        .unwrap_or_else(|| "All test targets failed".to_string());

    log::debug!(
        "[GlobalProxy] Test failed: {} -> {} ({}ms)",
        http_client::mask_url(&url),
        error_msg,
        latency
    );

    Ok(ProxyTestResult {
        success: false,
        latency_ms: latency,
        error: Some(error_msg),
    })
}

/// 获取当前出站代理状态
///
/// 返回当前是否启用了出站代理以及代理 URL。
#[tauri::command]
pub fn get_upstream_proxy_status() -> UpstreamProxyStatus {
    let url = http_client::get_current_proxy_url();
    UpstreamProxyStatus {
        enabled: url.is_some(),
        proxy_url: url.as_deref().map(http_client::mask_url),
    }
}

/// 出站代理状态信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamProxyStatus {
    /// 是否启用代理
    pub enabled: bool,
    /// 代理 URL
    pub proxy_url: Option<String>,
}

/// 检测到的代理信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProxy {
    /// 代理 URL
    pub url: String,
    /// 代理类型 (http/socks5)
    pub proxy_type: String,
    /// 端口
    pub port: u16,
}

/// 常见代理端口配置
/// 格式：(端口, 主要类型, 是否同时支持 http 和 socks5)
/// 对于 mixed 端口，会同时返回两种协议供用户选择
const PROXY_PORTS: &[(u16, &str, bool)] = &[
    (7890, "http", true),     // Clash (mixed mode)
    (7891, "socks5", false),  // Clash SOCKS only
    (1080, "socks5", false),  // 通用 SOCKS5
    (8080, "http", false),    // 通用 HTTP
    (8888, "http", false),    // Charles/Fiddler
    (3128, "http", false),    // Squid
    (10808, "socks5", false), // V2Ray SOCKS
    (10809, "http", false),   // V2Ray HTTP
];

/// 扫描本地代理
///
/// 检测常见端口是否有代理服务在运行。
/// 使用异步任务避免阻塞 UI 线程。
#[tauri::command]
pub async fn scan_local_proxies() -> Vec<DetectedProxy> {
    // 使用 spawn_blocking 避免阻塞主线程
    tokio::task::spawn_blocking(|| {
        let mut found = Vec::new();

        for &(port, primary_type, is_mixed) in PROXY_PORTS {
            let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
            if TcpStream::connect_timeout(&addr.into(), Duration::from_millis(100)).is_ok() {
                // 添加主要类型
                found.push(DetectedProxy {
                    url: format!("{primary_type}://127.0.0.1:{port}"),
                    proxy_type: primary_type.to_string(),
                    port,
                });
                // 对于 mixed 端口，同时添加另一种协议
                if is_mixed {
                    let alt_type = if primary_type == "http" {
                        "socks5"
                    } else {
                        "http"
                    };
                    found.push(DetectedProxy {
                        url: format!("{alt_type}://127.0.0.1:{port}"),
                        proxy_type: alt_type.to_string(),
                        port,
                    });
                }
            }
        }

        found
    })
    .await
    .unwrap_or_default()
}
