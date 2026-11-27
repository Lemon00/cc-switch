//! Exercise startup through the shipped CLI binary, including its daemon child.
//! Testing core::startup alone cannot detect a missing call in the CLI host.

use cc_switch_core::{app::AppType, config, database::Database, provider::Provider};
use serde_json::json;
use std::cell::Cell;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Fixture {
    home: PathBuf,
    config: PathBuf,
    command_count: Cell<usize>,
    daemon_started: bool,
}

impl Fixture {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home =
            std::env::temp_dir().join(format!("cli-outbound-{}-{unique}", std::process::id()));
        let config = home.join(".cc-switch");
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(home.join("tmp")).unwrap();
        eprintln!("CLI outbound proxy fixture: {}", home.display());
        Self {
            home,
            config,
            command_count: Cell::new(0),
            daemon_started: false,
        }
    }

    fn cli(&self, args: &[&str]) -> Result<String, String> {
        let count = self.command_count.get();
        self.command_count.set(count + 1);
        let stdout = self.home.join(format!("command-{count}.stdout"));
        let stderr = self.home.join(format!("command-{count}.stderr"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_cc-switch-cli"));
        command
            .args(["--app", "codex"])
            .args(args)
            .env("CC_SWITCH_TEST_HOME", &self.home)
            .env("CC_SWITCH_CONFIG_DIR", &self.config)
            .env("TEMP", self.home.join("tmp"))
            .env("TMP", self.home.join("tmp"))
            .env("TMPDIR", self.home.join("tmp"))
            .env_remove("CC_SWITCH_DAEMON_READY")
            // An inherited proxy must not make the missing-initialization case pass.
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).map_err(|e| e.to_string())?)
            .stderr(fs::File::create(&stderr).map_err(|e| e.to_string())?);
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
        ] {
            command.env_remove(key);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "CLI {args:?} timed out; logs: {}",
                    self.home.display()
                ));
            }
            thread::sleep(Duration::from_millis(20));
        };
        let output = fs::read_to_string(stdout).map_err(|e| e.to_string())?;
        let errors = fs::read_to_string(stderr).map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("CLI {args:?}: {status}\n{output}\n{errors}"));
        }
        Ok(output)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.daemon_started {
            if let Err(error) = self.cli(&["proxy", "disable"]) {
                eprintln!("CLI daemon cleanup failed: {error}");
            }
        }
        config::clear_app_config_dir_override();
    }
}

fn read_http(stream: &mut TcpStream) -> std::io::Result<(String, Vec<u8>)> {
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    let mut reader = BufReader::new(stream);
    let mut headers = String::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "HTTP headers incomplete",
            ));
        }
        if line == "\r\n" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        headers.push_str(&line);
        assert!(headers.len() < 64 * 1024, "unexpectedly large HTTP headers");
    }
    assert!(length < 1024 * 1024, "unexpectedly large HTTP body");
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok((headers, body))
}

struct OutboundProxy {
    url: String,
    received: mpsc::Receiver<(String, Vec<u8>)>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl OutboundProxy {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (send, received) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Accepted sockets inherit the listener's nonblocking mode on Windows.
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let request = read_http(&mut stream).unwrap();
                        let body = json!({"id":"resp_outbound_fixture", "object":"response",
                            "status":"completed", "model":"fixture", "output":[],
                            "usage":{"input_tokens":12,"output_tokens":5,"total_tokens":17}})
                        .to_string();
                        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                        send.send(request).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("mock outbound proxy: {error}"),
                }
            }
        });
        Self {
            url,
            received,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for OutboundProxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
fn cli_loads_saved_outbound_proxy_before_starting_daemon() {
    // This integration executable has one test, so its core configuration override
    // cannot race another test. The CLI and daemon each get their own process state.
    let mut fixture = Fixture::new();
    config::set_app_config_dir_override(fixture.config.clone());
    let db = Database::init().unwrap();
    let outbound = OutboundProxy::start();
    let port_reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = port_reservation.local_addr().unwrap().port();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut proxy = db.get_proxy_config().await.unwrap();
        proxy.listen_port = port;
        proxy.max_retries = 0;
        db.update_proxy_config(proxy).await.unwrap();
    });
    db.set_global_proxy_url(Some(&outbound.url)).unwrap();
    let provider = Provider::with_id(
        "outbound-fixture".into(),
        "Outbound fixture".into(),
        json!({
            "auth": {"OPENAI_API_KEY": "fixture-upstream-key"},
            "config": "model_provider = \"custom\"\nmodel = \"fixture\"\n[model_providers.custom]\nname = \"Fixture\"\nbase_url = \"http://cli-outbound-fixture.invalid/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n"
        }),
        None,
    );
    db.save_provider(&AppType::Codex, &provider).unwrap();
    fixture
        .cli(&["provider", "switch", "outbound-fixture"])
        .unwrap();
    fs::write(
        fixture.config.join("live-state.json"),
        json!({"version":1,"apps":{"codex":{
            "mode":"proxy","attached":false,"proxy_route":"outbound-fixture"
        }}})
        .to_string(),
    )
    .unwrap();
    drop(port_reservation);
    fixture.daemon_started = true;
    fixture.cli(&["proxy", "enable"]).unwrap();

    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let body = json!({"model":"fixture", "input":"outbound startup regression", "stream":false})
        .to_string();
    write!(client, "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer fixture-local\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let (headers, response) = read_http(&mut client).unwrap();
    assert!(
        headers.starts_with("HTTP/1.1 200"),
        "{headers}\n{}",
        String::from_utf8_lossy(&response)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response).unwrap()["id"],
        "resp_outbound_fixture"
    );
    let (forwarded, request) = outbound
        .received
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(
        forwarded.starts_with("POST http://cli-outbound-fixture.invalid/v1/responses HTTP/1.1\r\n"),
        "{forwarded}"
    );
    assert!(
        forwarded
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-upstream-key\r\n"),
        "{forwarded}"
    );
    assert!(
        !forwarded.contains("fixture-local"),
        "local token leaked upstream"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&request).unwrap()["input"],
        "outbound startup regression"
    );
    assert_eq!(
        db.get_global_proxy_url().unwrap().as_deref(),
        Some(outbound.url.as_str())
    );
    fixture.cli(&["proxy", "disable"]).unwrap();
    fixture.daemon_started = false;
    assert!(!fixture.config.join("cli-proxy.pid").exists());
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());

    // Exercise the ordinary command entry too: malformed configuration is cleared,
    // while a syntactically valid but unavailable proxy must stay saved.
    let unavailable = TcpListener::bind("127.0.0.1:0").unwrap();
    let unavailable_url = format!("http://{}", unavailable.local_addr().unwrap());
    drop(unavailable);
    db.set_global_proxy_url(Some(&unavailable_url)).unwrap();
    fixture.cli(&["provider", "list"]).unwrap();
    assert_eq!(
        db.get_global_proxy_url().unwrap().as_deref(),
        Some(unavailable_url.as_str())
    );
    db.set_global_proxy_url(Some("invalid://proxy")).unwrap();
    fixture.cli(&["provider", "list"]).unwrap();
    assert!(db.get_global_proxy_url().unwrap().is_none());
}
