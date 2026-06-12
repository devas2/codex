//! linux.do latest-topic footer support.

use std::fs::File;
use std::net::TcpListener;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use chrono::DateTime;
use chrono::Utc;
use futures::SinkExt;
use futures::StreamExt;
use ratatui::style::Stylize;
use ratatui::text::Line;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tokio::process::Child;
use tokio::process::Command;
use tokio::time::Instant;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const LINUX_DO_LATEST_URL: &str = "https://linux.do/latest.json";
const PROFILE_DIR_NAME: &str = "linux-do-webview-profile";
const PROFILE_LOCK_FILE: &str = "webview.lock";
const DEVTOOLS_READY_TIMEOUT: Duration = Duration::from_secs(10);
const HEADLESS_PAGE_TIMEOUT: Duration = Duration::from_secs(20);
const VISIBLE_CHALLENGE_TIMEOUT: Duration = Duration::from_secs(300);
const PAGE_POLL_INTERVAL: Duration = Duration::from_millis(750);

type CdpSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinuxDoLatestPost {
    pub(crate) author: String,
    pub(crate) title: String,
    pub(crate) last_posted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LinuxDoLatestFetchOutcome {
    Post(LinuxDoLatestPost),
    Busy,
}

#[derive(Debug, Error)]
pub(crate) enum LinuxDoLatestError {
    #[error("linux.do returned no topics")]
    Empty,
    #[error("failed to parse linux.do latest JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to prepare linux.do webview profile: {0}")]
    Io(#[from] std::io::Error),
    #[error("no Chrome or Edge browser executable was found")]
    BrowserNotFound,
    #[error("failed to start browser: {0}")]
    BrowserStart(std::io::Error),
    #[error("browser DevTools endpoint did not become ready")]
    DevtoolsNotReady,
    #[error("browser DevTools request failed: {0}")]
    DevtoolsHttp(#[from] reqwest::Error),
    #[error("browser DevTools websocket failed: {0}")]
    DevtoolsWebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("browser DevTools response was invalid: {0}")]
    DevtoolsProtocol(String),
    #[error("complete the linux.do Cloudflare check in the browser window")]
    ChallengePending,
    #[error("timed out waiting for the linux.do Cloudflare check")]
    ChallengeTimedOut,
}

#[derive(Debug, Deserialize)]
struct LatestResponse {
    topic_list: TopicList,
}

#[derive(Debug, Deserialize)]
struct TopicList {
    topics: Vec<Topic>,
}

#[derive(Debug, Deserialize)]
struct Topic {
    title: String,
    last_poster_username: Option<String>,
    last_posted_at: Option<DateTime<Utc>>,
    bumped_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    visible: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DevToolsTarget {
    id: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
}

#[derive(Debug, Deserialize)]
struct DevToolsVersion {
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PageTextSnapshot {
    #[allow(dead_code)]
    url: String,
    #[allow(dead_code)]
    title: String,
    #[serde(rename = "readyState")]
    ready_state: String,
    text: String,
}

struct WebviewProfileLock {
    _file: File,
}

struct BrowserSession {
    child: Child,
    port: u16,
    client: Client,
    closed: bool,
}

impl Drop for BrowserSession {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.child.start_kill();
        }
    }
}

pub(crate) fn loading_line() -> Line<'static> {
    vec![
        "[linux.do]".cyan().bold(),
        " ".into(),
        "loading latest topic...".dim(),
    ]
    .into()
}

pub(crate) fn error_line(message: &str) -> Line<'static> {
    vec![
        "[linux.do]".red().bold(),
        " ".into(),
        message.to_string().dim(),
    ]
    .into()
}

pub(crate) fn busy_line() -> Line<'static> {
    vec![
        "[linux.do]".cyan().bold(),
        " ".into(),
        "shared webview is refreshing...".dim(),
    ]
    .into()
}

pub(crate) fn line_for_post(post: &LinuxDoLatestPost, now: DateTime<Utc>) -> Line<'static> {
    vec![
        "[linux.do]".cyan().bold(),
        " ".into(),
        post.author.clone().green(),
        " | ".dim(),
        post.title.clone().magenta(),
        " | ".dim(),
        ago_label(post.last_posted_at, now).dim(),
    ]
    .into()
}

pub(crate) async fn fetch_latest_via_webview(
    codex_home: PathBuf,
) -> Result<LinuxDoLatestFetchOutcome, LinuxDoLatestError> {
    let profile_dir = codex_home.join(PROFILE_DIR_NAME);
    let Some(_profile_lock) = try_lock_webview_profile(&profile_dir)? else {
        return Ok(LinuxDoLatestFetchOutcome::Busy);
    };

    match fetch_with_browser(&profile_dir, BrowserVisibility::Headless).await {
        Ok(post) => Ok(LinuxDoLatestFetchOutcome::Post(post)),
        Err(LinuxDoLatestError::ChallengePending) => {
            let post = fetch_after_visible_challenge(&profile_dir).await?;
            Ok(LinuxDoLatestFetchOutcome::Post(post))
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn parse_latest_post(json: &str) -> Result<LinuxDoLatestPost, LinuxDoLatestError> {
    let response: LatestResponse = serde_json::from_str(json)?;
    let topic = response
        .topic_list
        .topics
        .into_iter()
        .find(|topic| topic.visible.unwrap_or(true))
        .ok_or(LinuxDoLatestError::Empty)?;
    Ok(LinuxDoLatestPost {
        author: topic
            .last_poster_username
            .filter(|author| !author.trim().is_empty())
            .unwrap_or_else(|| "unknown".to_string()),
        title: topic.title,
        last_posted_at: topic
            .last_posted_at
            .or(topic.bumped_at)
            .unwrap_or(topic.created_at),
    })
}

fn ago_label(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let elapsed = now.signed_duration_since(then);
    if elapsed.num_seconds() < 60 {
        return "just now".to_string();
    }
    if elapsed.num_minutes() < 60 {
        return format!("{}m ago", elapsed.num_minutes());
    }
    if elapsed.num_hours() < 24 {
        return format!("{}h ago", elapsed.num_hours());
    }
    if elapsed.num_days() < 30 {
        return format!("{}d ago", elapsed.num_days());
    }
    if elapsed.num_days() < 365 {
        return format!("{}mo ago", elapsed.num_days() / 30);
    }
    format!("{}y ago", elapsed.num_days() / 365)
}

fn try_lock_webview_profile(
    profile_dir: &Path,
) -> Result<Option<WebviewProfileLock>, LinuxDoLatestError> {
    std::fs::create_dir_all(profile_dir)?;
    let lock_file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(profile_dir.join(PROFILE_LOCK_FILE))?;
    match lock_file.try_lock() {
        Ok(()) => Ok(Some(WebviewProfileLock { _file: lock_file })),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(err) => Err(LinuxDoLatestError::Io(err.into())),
    }
}

#[derive(Clone, Copy, Debug)]
enum BrowserVisibility {
    Headless,
    Visible,
}

async fn fetch_with_browser(
    profile_dir: &Path,
    visibility: BrowserVisibility,
) -> Result<LinuxDoLatestPost, LinuxDoLatestError> {
    let mut browser = BrowserSession::launch(profile_dir, visibility).await?;
    let read_timeout = match visibility {
        BrowserVisibility::Headless => HEADLESS_PAGE_TIMEOUT,
        BrowserVisibility::Visible => VISIBLE_CHALLENGE_TIMEOUT,
    };
    let read_result = read_latest_from_browser_page(&browser, read_timeout).await;
    browser.close().await;
    read_result
}

async fn fetch_after_visible_challenge(
    profile_dir: &Path,
) -> Result<LinuxDoLatestPost, LinuxDoLatestError> {
    match fetch_with_browser(profile_dir, BrowserVisibility::Visible).await {
        Err(LinuxDoLatestError::ChallengePending) => Err(LinuxDoLatestError::ChallengeTimedOut),
        other => other,
    }
}

async fn read_latest_from_browser_page(
    browser: &BrowserSession,
    read_timeout: Duration,
) -> Result<LinuxDoLatestPost, LinuxDoLatestError> {
    let target = browser.create_target(LINUX_DO_LATEST_URL).await?;
    let mut ws = connect_target(&target.web_socket_debugger_url).await?;
    let deadline = Instant::now() + read_timeout;
    loop {
        if let Some(post) = maybe_read_latest_from_page(&mut ws).await? {
            let _ = browser.close_target(&target.id).await;
            return Ok(post);
        }
        if Instant::now() >= deadline {
            let _ = browser.close_target(&target.id).await;
            return Err(LinuxDoLatestError::ChallengePending);
        }
        sleep(PAGE_POLL_INTERVAL).await;
    }
}

async fn maybe_read_latest_from_page(
    ws: &mut CdpSocket,
) -> Result<Option<LinuxDoLatestPost>, LinuxDoLatestError> {
    let snapshot = evaluate_page_text(ws).await?;
    let trimmed = snapshot.text.trim();
    if trimmed.is_empty() || snapshot.ready_state == "loading" {
        return Ok(None);
    }
    match parse_latest_post(trimmed) {
        Ok(post) => Ok(Some(post)),
        Err(LinuxDoLatestError::Parse(_)) | Err(LinuxDoLatestError::Empty) => Ok(None),
        Err(err) => Err(err),
    }
}

async fn evaluate_page_text(ws: &mut CdpSocket) -> Result<PageTextSnapshot, LinuxDoLatestError> {
    let result = send_cdp_command(
        ws,
        1,
        "Runtime.evaluate",
        serde_json::json!({
            "expression": r#"JSON.stringify({
                url: location.href,
                title: document.title || "",
                readyState: document.readyState,
                text: document.body ? document.body.innerText : ""
            })"#,
            "awaitPromise": true,
            "returnByValue": true
        }),
    )
    .await?;
    let value = result
        .get("result")
        .and_then(|runtime_result| runtime_result.get("value"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            LinuxDoLatestError::DevtoolsProtocol(
                "Runtime.evaluate returned no string value".to_string(),
            )
        })?;
    serde_json::from_str(value).map_err(LinuxDoLatestError::Parse)
}

async fn connect_target(url: &str) -> Result<CdpSocket, LinuxDoLatestError> {
    let (socket, _) = connect_async(url).await?;
    Ok(socket)
}

async fn send_cdp_command(
    ws: &mut CdpSocket,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, LinuxDoLatestError> {
    ws.send(Message::Text(
        serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        })
        .to_string()
        .into(),
    ))
    .await?;

    loop {
        let message = timeout(Duration::from_secs(10), ws.next())
            .await
            .map_err(|_| {
                LinuxDoLatestError::DevtoolsProtocol(format!(
                    "timed out waiting for CDP response to {method}"
                ))
            })?
            .ok_or_else(|| {
                LinuxDoLatestError::DevtoolsProtocol("CDP websocket closed".to_string())
            })??;
        let Message::Text(text) = message else {
            continue;
        };
        let response: Value = serde_json::from_str(&text).map_err(LinuxDoLatestError::Parse)?;
        if response.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if let Some(error) = response.get("error") {
            return Err(LinuxDoLatestError::DevtoolsProtocol(error.to_string()));
        }
        return response.get("result").cloned().ok_or_else(|| {
            LinuxDoLatestError::DevtoolsProtocol(format!("CDP response to {method} had no result"))
        });
    }
}

impl BrowserSession {
    async fn launch(
        profile_dir: &Path,
        visibility: BrowserVisibility,
    ) -> Result<Self, LinuxDoLatestError> {
        let browser = find_browser_executable().ok_or(LinuxDoLatestError::BrowserNotFound)?;
        let port = reserve_local_port()?;
        let mut command = Command::new(browser);
        command
            .arg(format!("--user-data-dir={}", profile_dir.display()))
            .arg(format!("--remote-debugging-port={port}"))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking");
        if matches!(visibility, BrowserVisibility::Headless) {
            command.arg("--headless=new").arg("--disable-gpu");
        } else {
            command.arg("--new-window");
        }
        command.arg("about:blank");
        let child = command.spawn().map_err(LinuxDoLatestError::BrowserStart)?;
        let session = Self {
            child,
            port,
            client: Client::new(),
            closed: false,
        };
        session.wait_for_devtools().await?;
        Ok(session)
    }

    async fn wait_for_devtools(&self) -> Result<(), LinuxDoLatestError> {
        let deadline = Instant::now() + DEVTOOLS_READY_TIMEOUT;
        while Instant::now() < deadline {
            if self.version().await.is_ok() {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
        Err(LinuxDoLatestError::DevtoolsNotReady)
    }

    async fn version(&self) -> Result<DevToolsVersion, LinuxDoLatestError> {
        let url = format!("http://127.0.0.1:{}/json/version", self.port);
        Ok(self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<DevToolsVersion>()
            .await?)
    }

    async fn create_target(&self, url: &str) -> Result<DevToolsTarget, LinuxDoLatestError> {
        let url = format!(
            "http://127.0.0.1:{}/json/new?{}",
            self.port,
            urlencoding::encode(url)
        );
        Ok(self
            .client
            .put(url)
            .send()
            .await?
            .error_for_status()?
            .json::<DevToolsTarget>()
            .await?)
    }

    async fn close_target(&self, target_id: &str) -> Result<(), LinuxDoLatestError> {
        let url = format!("http://127.0.0.1:{}/json/close/{target_id}", self.port);
        self.client.get(url).send().await?.error_for_status()?;
        Ok(())
    }

    async fn close(&mut self) {
        if self.closed {
            return;
        }
        if let Ok(version) = self.version().await
            && let Some(ws_url) = version.web_socket_debugger_url.as_deref()
            && let Ok(mut ws) = connect_target(ws_url).await
        {
            let _ = send_cdp_command(&mut ws, 99, "Browser.close", serde_json::json!({})).await;
        }
        match timeout(Duration::from_secs(3), self.child.wait()).await {
            Ok(Ok(_)) => {}
            _ => {
                let _ = self.child.start_kill();
                let _ = self.child.wait().await;
            }
        }
        self.closed = true;
    }
}

fn reserve_local_port() -> Result<u16, LinuxDoLatestError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn find_browser_executable() -> Option<PathBuf> {
    browser_candidates().into_iter().find(|path| path.is_file())
}

fn browser_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(target_os = "windows") {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
            if let Some(root) = std::env::var_os(var) {
                let root = PathBuf::from(root);
                paths.push(root.join("Microsoft/Edge/Application/msedge.exe"));
                paths.push(root.join("Google/Chrome/Application/chrome.exe"));
            }
        }
        extend_path_candidates(&mut paths, &["msedge.exe", "chrome.exe", "chromium.exe"]);
    } else if cfg!(target_os = "macos") {
        paths.push(PathBuf::from(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        ));
        paths.push(PathBuf::from(
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ));
        paths.push(PathBuf::from(
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ));
        extend_path_candidates(
            &mut paths,
            &[
                "google-chrome",
                "microsoft-edge",
                "chromium",
                "chromium-browser",
            ],
        );
    } else {
        extend_path_candidates(
            &mut paths,
            &[
                "google-chrome",
                "google-chrome-stable",
                "microsoft-edge",
                "microsoft-edge-stable",
                "chromium",
                "chromium-browser",
            ],
        );
    }
    paths
}

fn extend_path_candidates(paths: &mut Vec<PathBuf>, names: &[&str]) {
    let Some(path_var) = std::env::var_os("PATH") else {
        return;
    };
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            paths.push(dir.join(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_latest_topic() {
        let json = r#"{
            "topic_list": {
                "topics": [{
                    "title": "最新帖子标题",
                    "created_at": "2026-06-11T07:00:00Z",
                    "last_posted_at": "2026-06-11T07:43:25Z",
                    "last_poster_username": "neo",
                    "visible": true
                }]
            }
        }"#;

        let post = parse_latest_post(json).expect("post should parse");

        assert_eq!(
            post,
            LinuxDoLatestPost {
                author: "neo".to_string(),
                title: "最新帖子标题".to_string(),
                last_posted_at: Utc.with_ymd_and_hms(2026, 6, 11, 7, 43, 25).unwrap(),
            }
        );
    }

    #[test]
    fn renders_colored_line_text() {
        let post = LinuxDoLatestPost {
            author: "neo".to_string(),
            title: "最新帖子标题".to_string(),
            last_posted_at: Utc.with_ymd_and_hms(2026, 6, 11, 7, 40, 0).unwrap(),
        };
        let now = Utc.with_ymd_and_hms(2026, 6, 11, 7, 45, 0).unwrap();

        let text = line_for_post(&post, now)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(text, "[linux.do] neo | 最新帖子标题 | 5m ago");
    }
}
