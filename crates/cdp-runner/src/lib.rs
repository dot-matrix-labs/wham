use std::env;
use serde_json::Value;
use std::io::ErrorKind;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub mod pixel_diff;

pub struct Config {
    pub url: String,
    pub port: u16,
    pub headless: bool,
    pub start_server: bool,
    pub start_chrome: bool,
    /// Optional directory to write PNG screenshots into.
    /// Populated from the `CDP_SCREENSHOT_DIR` env var.
    pub screenshot_dir: Option<std::path::PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            url: env::var("CDP_URL").unwrap_or_else(|_| "http://127.0.0.1:8000/index.html".to_string()),
            port: env::var("CDP_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(9222),
            headless: env::var("CDP_HEADLESS").unwrap_or_else(|_| "1".to_string()) != "0",
            start_server: env::var("CDP_NO_SERVER").unwrap_or_else(|_| "0".to_string()) != "1",
            start_chrome: env::var("CDP_NO_CHROME").unwrap_or_else(|_| "0".to_string()) != "1",
            screenshot_dir: env::var("CDP_SCREENSHOT_DIR").ok().map(std::path::PathBuf::from),
        }
    }
}

/// A live browser session. Created with `BrowserSession::launch`, dropped to
/// clean up Chrome and the file server.
pub struct BrowserSession {
    ws: WebSocket,
    pub screenshot_dir: Option<std::path::PathBuf>,
    chrome: Option<Child>,
    server: Option<Child>,
    profile_dir: Option<std::path::PathBuf>,
    pub url: String,
}

impl BrowserSession {
    /// Launch Chrome (and optionally a local HTTP server), connect the CDP
    /// WebSocket, and enable the necessary domains.  Returns `Err` if Chrome
    /// is not installed — callers should translate that into a graceful skip.
    pub fn launch(config: &Config) -> Result<Self, String> {
        let mut server = None;
        if config.start_server {
            server = Some(start_server()?);
            thread::sleep(Duration::from_millis(200));
        }

        let (chrome, profile_dir, ws_url) = if config.start_chrome {
            let (child, dir, ws_url) =
                start_chrome_with_retry(config.port, config.headless, &config.url)?;
            (Some(child), Some(dir), ws_url)
        } else {
            return Err("CDP_NO_CHROME is set; chromium must be started explicitly".to_string());
        };

        let mut ws = WebSocket::connect(&ws_url)?;
        {
            let mut cdp = CdpClient::new(&mut ws);
            cdp.send("Page.enable", "{}")?;
            cdp.send("Runtime.enable", "{}")?;
            cdp.send("Input.enable", "{}")?;
        }

        Ok(Self {
            ws,
            screenshot_dir: config.screenshot_dir.clone(),
            chrome,
            server,
            profile_dir,
            url: config.url.clone(),
        })
    }

    /// Navigate to the configured URL and wait for the app to be ready.
    /// After this call the app home screen is visible.
    pub fn navigate_to_app(&mut self) -> Result<(), String> {
        let url = self.url.clone();
        let mut cdp = CdpClient::new(&mut self.ws);
        cdp.send(
            "Page.navigate",
            &format!("{{\"url\":\"{}\"}}", url.replace('"', "\\\"")),
        )?;
        wait_for_eval_contains(
            &mut cdp,
            "document.readyState",
            "complete",
            Duration::from_secs(3),
        )?;
        wait_for_eval_contains(
            &mut cdp,
            "window.__app ? \"ready\" : \"\"",
            "ready",
            Duration::from_secs(3),
        )?;
        wait_for_a11y(&mut cdp, "GPU Forms UI", Duration::from_secs(3))?;
        Ok(())
    }

    /// Click the Dynamic Validation link and wait for the form to appear.
    /// Assumes `navigate_to_app` has already been called.
    pub fn open_dynamic_form(&mut self) -> Result<(), String> {
        let mut cdp = CdpClient::new(&mut self.ws);
        click_named(&mut cdp, "Dynamic Validation")?;
        thread::sleep(Duration::from_millis(200));
        wait_for_a11y(&mut cdp, "Username", Duration::from_secs(3))?;
        Ok(())
    }

    /// Type valid values into the Username and Age fields.
    /// Assumes the dynamic form is already open.
    pub fn fill_form_valid_input(&mut self) -> Result<(), String> {
        let mut cdp = CdpClient::new(&mut self.ws);
        click_named(&mut cdp, "Username")?;
        cdp.eval_void("window.__app.handle_text_input('user1')")?;
        tick_app(&mut cdp)?;
        click_named(&mut cdp, "Age")?;
        cdp.eval_void("window.__app.handle_text_input('18')")?;
        tick_app(&mut cdp)?;
        Ok(())
    }

    /// Click Submit and wait for the submitting indicator.
    /// Assumes the form has been filled with valid input.
    pub fn click_submit_and_wait_for_submitting(&mut self) -> Result<(), String> {
        let mut cdp = CdpClient::new(&mut self.ws);
        click_named(&mut cdp, "Submit Profile")?;
        wait_for_a11y(&mut cdp, "Submitting...", Duration::from_secs(2))?;
        Ok(())
    }

    /// Wait for the success confirmation after a submit completes.
    /// Assumes the submitting state is already showing.
    pub fn wait_for_success(&mut self) -> Result<(), String> {
        let mut cdp = CdpClient::new(&mut self.ws);
        thread::sleep(Duration::from_millis(1100));
        wait_for_a11y(&mut cdp, "Saved successfully.", Duration::from_secs(3))?;
        Ok(())
    }

    /// Click Submit a second time and wait for the server-error rollback message.
    /// Assumes the success state is currently showing.
    pub fn click_submit_again_and_wait_for_rollback(&mut self) -> Result<(), String> {
        let mut cdp = CdpClient::new(&mut self.ws);
        click_named(&mut cdp, "Submit Profile")?;
        thread::sleep(Duration::from_millis(1100));
        wait_for_a11y(&mut cdp, "Server error, rolled back.", Duration::from_secs(3))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Composite helpers used by individual tests to reach a known start state
    // -----------------------------------------------------------------------

    /// Fresh load → app home screen.
    pub fn setup_initial_load(&mut self) -> Result<(), String> {
        self.navigate_to_app()
    }

    /// Fresh load → dynamic validation form open.
    pub fn setup_dynamic_form(&mut self) -> Result<(), String> {
        self.navigate_to_app()?;
        self.open_dynamic_form()
    }

    /// Fresh load → form filled with valid input.
    pub fn setup_filled_form(&mut self) -> Result<(), String> {
        self.setup_dynamic_form()?;
        self.fill_form_valid_input()
    }

    /// Fresh load → submitting indicator showing.
    pub fn setup_submitting_state(&mut self) -> Result<(), String> {
        self.setup_filled_form()?;
        self.click_submit_and_wait_for_submitting()
    }

    /// Fresh load → success confirmation showing.
    pub fn setup_success_state(&mut self) -> Result<(), String> {
        self.setup_submitting_state()?;
        self.wait_for_success()
    }

    /// Fresh load → rollback/server-error message showing.
    pub fn setup_rollback_state(&mut self) -> Result<(), String> {
        self.setup_success_state()?;
        self.click_submit_again_and_wait_for_rollback()
    }

    /// Take a screenshot and optionally run a visual regression check.
    /// Returns any regression errors via `regression_errors`.
    pub fn take_screenshot(
        &mut self,
        name: &str,
        regression_errors: &mut Vec<String>,
    ) -> Result<(), String> {
        let mut cdp = CdpClient::new(&mut self.ws);
        take_screenshot(&mut cdp, &self.screenshot_dir, name, regression_errors)
    }
}

impl Drop for BrowserSession {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.chrome {
            let _ = child.kill();
        }
        if let Some(ref mut child) = self.server {
            let _ = child.kill();
        }
        if let Some(ref dir) = self.profile_dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Capture a full-page PNG screenshot via CDP, write it to disk, and
/// optionally compare against a visual baseline.
///
/// If `screenshot_dir` is `None` the call is a no-op (returns `Ok(())`).
/// When `CDP_UPDATE_BASELINES=1`, the captured screenshot is copied to the
/// baseline directory instead of being compared.
fn take_screenshot(
    cdp: &mut CdpClient,
    screenshot_dir: &Option<std::path::PathBuf>,
    name: &str,
    regression_errors: &mut Vec<String>,
) -> Result<(), String> {
    let dir = match screenshot_dir {
        Some(d) => d,
        None => return Ok(()),
    };
    // Ask Chromium for a PNG screenshot (base64 encoded).
    let id = cdp.next_id;
    cdp.next_id += 1;
    let msg = format!(
        "{{\"id\":{},\"method\":\"Page.captureScreenshot\",\"params\":{{\"format\":\"png\",\"fromSurface\":true}}}}",
        id
    );
    cdp.ws.send_text(&msg)?;
    let resp = cdp.wait_for_id(id, Duration::from_secs(10))?;
    let b64 = resp
        .pointer("/result/data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "screenshot data missing".to_string())?;
    let bytes = base64_decode(b64).map_err(|e| format!("screenshot decode: {}", e))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("screenshot dir: {}", e))?;
    let path = dir.join(format!("{}.png", name));
    std::fs::write(&path, &bytes).map_err(|e| format!("screenshot write: {}", e))?;
    println!("[screenshot] saved {}", path.display());

    // Visual regression: update baseline or compare.
    if pixel_diff::should_update_baselines() {
        pixel_diff::update_baseline(&path, name)?;
    } else {
        match pixel_diff::compare_screenshot(&bytes, name, dir) {
            Ok(_) => {} // pass or no baseline
            Err(e) => {
                // Collect regression errors instead of failing immediately,
                // so all screenshots are captured before the test reports.
                regression_errors.push(e);
            }
        }
    }

    Ok(())
}

/// Minimal base64 decoder — no external dependencies.
fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    const TABLE: &[u8; 128] = b"\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\
                                 \x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\
                                 \x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x40\x3e\x40\x40\x40\x3f\
                                 \x34\x35\x36\x37\x38\x39\x3a\x3b\x3c\x3d\x40\x40\x40\x40\x40\x40\
                                 \x40\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\
                                 \x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x40\x40\x40\x40\x40\
                                 \x40\x1a\x1b\x1c\x1d\x1e\x1f\x20\x21\x22\x23\x24\x25\x26\x27\x28\
                                 \x29\x2a\x2b\x2c\x2d\x2e\x2f\x30\x31\x32\x33\x40\x40\x40\x40\x40";
    let input = input.trim();
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let bytes: Vec<u8> = input.bytes().filter(|b| *b != b'\n' && *b != b'\r').collect();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let a = bytes[i] as usize;
        let b = bytes[i + 1] as usize;
        let c = bytes[i + 2] as usize;
        let d = bytes[i + 3] as usize;
        if a >= 128 || b >= 128 || c >= 128 || d >= 128 {
            return Err("invalid base64 char");
        }
        let va = TABLE[a];
        let vb = TABLE[b];
        let vc = TABLE[c];
        let vd = TABLE[d];
        if va == 0x40 || vb == 0x40 {
            return Err("invalid base64 char");
        }
        out.push((va << 2) | (vb >> 4));
        if bytes[i + 2] != b'=' {
            if vc == 0x40 {
                return Err("invalid base64 char");
            }
            out.push(((vb & 0x0f) << 4) | (vc >> 2));
        }
        if bytes[i + 3] != b'=' {
            if vd == 0x40 {
                return Err("invalid base64 char");
            }
            out.push(((vc & 0x03) << 6) | vd);
        }
        i += 4;
    }
    Ok(out)
}

pub fn run(config: Config) -> Result<(), String> {
    let mut session = BrowserSession::launch(&config)?;
    let mut regression_errors: Vec<String> = Vec::new();

    // Step 1 — initial load
    session.navigate_to_app()?;
    session.take_screenshot("01_loaded", &mut regression_errors)?;

    // Step 2 — open the Dynamic Validation form
    session.open_dynamic_form()?;
    session.take_screenshot("02_dynamic_form", &mut regression_errors)?;

    // Step 3 — fill in valid input
    session.fill_form_valid_input()?;
    session.take_screenshot("03_filled_form", &mut regression_errors)?;

    // Step 4 — submit and capture the submitting indicator
    session.click_submit_and_wait_for_submitting()?;
    session.take_screenshot("04_submitting", &mut regression_errors)?;

    // Step 5 — wait for success confirmation
    session.wait_for_success()?;
    session.take_screenshot("05_success", &mut regression_errors)?;

    // Step 6 — re-submit to trigger server error rollback
    session.click_submit_again_and_wait_for_rollback()?;
    session.take_screenshot("06_rollback", &mut regression_errors)?;

    // Report visual regression failures after all screenshots are captured.
    if !regression_errors.is_empty() {
        let count = regression_errors.len();
        let details = regression_errors.join("\n  ");
        return Err(format!(
            "{} visual regression(s) detected:\n  {}",
            count, details
        ));
    }

    Ok(())
}

fn start_server() -> Result<Child, String> {
    match spawn_server("python3") {
        Ok(child) => Ok(child),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            spawn_server("python").map_err(|e| format!("server start failed: {}", e))
        }
        Err(err) => Err(format!("server start failed: {}", err)),
    }
}

fn spawn_server(bin: &str) -> std::io::Result<Child> {
    let mut web_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    web_root.push("..");
    web_root.push("..");
    web_root.push("examples");
    web_root.push("web");
    let mut cmd = Command::new(bin);
    cmd.arg("-m").arg("http.server").arg("8000");
    cmd.current_dir(web_root);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn()
}

fn start_chrome_with_retry(
    port: u16,
    headless: bool,
    target_url: &str,
) -> Result<(Child, std::path::PathBuf, String), String> {
    if !headless {
        let (child, dir) = start_chrome(port, None)?;
        let ws_url = wait_for_ws_url(port, target_url, Duration::from_secs(5))?;
        return Ok((child, dir, ws_url));
    }

    let headless_args = ["--headless=new", "--headless"];
    let mut last_err = None;
    for headless_arg in headless_args {
        let (mut child, dir) = start_chrome(port, Some(headless_arg))?;
        match wait_for_ws_url(port, target_url, Duration::from_secs(5)) {
            Ok(ws_url) => return Ok((child, dir, ws_url)),
            Err(err) => {
                let _ = child.kill();
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| "chrome start failed".to_string()))
}

fn start_chrome(port: u16, headless_arg: Option<&str>) -> Result<(Child, std::path::PathBuf), String> {
    let chrome_bin = find_chrome_bin().ok_or_else(|| "chrome binary not found".to_string())?;
    let profile_dir = new_profile_dir()?;
    let mut cmd = Command::new(chrome_bin);
    cmd.arg(format!("--remote-debugging-port={}", port))
        .arg("--window-size=1280,720")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    if let Some(arg) = headless_arg {
        cmd.arg(arg);
    }
    cmd.arg("about:blank");
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let child = cmd.spawn().map_err(|e| format!("chrome start failed: {}", e))?;
    Ok((child, profile_dir))
}

fn new_profile_dir() -> Result<std::path::PathBuf, String> {
    let mut dir = env::temp_dir();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("time error: {}", e))?
        .as_millis();
    let pid = std::process::id();
    dir.push(format!("cdp-runner-profile-{}-{}", pid, now));
    std::fs::create_dir_all(&dir).map_err(|e| format!("profile dir: {}", e))?;
    Ok(dir)
}

fn find_chrome_bin() -> Option<String> {
    if let Ok(bin) = env::var("CDP_CHROME_BIN") {
        return Some(bin);
    }
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "google-chrome",
        "chromium",
        "chrome",
    ];
    for cand in candidates {
        if which(cand).is_some() {
            return Some(cand.to_string());
        }
    }
    None
}

fn which(bin: &str) -> Option<String> {
    if bin.contains('/') {
        if std::path::Path::new(bin).exists() {
            return Some(bin.to_string());
        }
        return None;
    }
    let path = env::var("PATH").ok()?;
    for entry in path.split(':') {
        let candidate = format!("{}/{}", entry, bin);
        if std::path::Path::new(&candidate).exists() {
            return Some(candidate);
        }
    }
    None
}

fn fetch_ws_url(port: u16, target_url: &str) -> Result<String, String> {
    let list_resp = http_get("127.0.0.1", port, "/json/list")?;
    let list_body = http_body(&list_resp);
    let list_json: Value = serde_json::from_str(list_body)
        .map_err(|e| format!("ws list parse error: {}", e))?;
    let list = list_json
        .as_array()
        .ok_or_else(|| "ws list not array".to_string())?;

    let mut fallback_page: Option<String> = None;
    let mut fallback_any: Option<String> = None;

    for entry in list {
        if let Some(ws) = extract_ws_from_target(entry) {
            let url = entry.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if url == target_url || url == target_url.trim_end_matches("index.html") {
                return Ok(ws);
            }
            if entry_type == "page" {
                if fallback_page.is_none() {
                    fallback_page = Some(ws.clone());
                }
                if url.starts_with("http://127.0.0.1:8000") {
                    return Ok(ws);
                }
            }
            if fallback_any.is_none() {
                fallback_any = Some(ws);
            }
        }
    }

    if let Some(ws) = fallback_page.or(fallback_any) {
        return Ok(ws);
    }

    let mut preview_list = list_body.replace('\n', " ");
    if preview_list.len() > 200 {
        preview_list.truncate(200);
    }
    Err(format!("ws url not found (list: {})", preview_list))
}

fn http_body(resp: &str) -> &str {
    if let Some(idx) = resp.find("\r\n\r\n") {
        &resp[idx + 4..]
    } else {
        resp
    }
}

fn wait_for_ws_url(port: u16, target_url: &str, timeout: Duration) -> Result<String, String> {
    let start = Instant::now();
    loop {
        match fetch_ws_url(port, target_url) {
            Ok(url) => return Ok(url),
            Err(err) => {
                if start.elapsed() >= timeout {
                    return Err(err);
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn http_get(host: &str, port: u16, path: &str) -> Result<String, String> {
    http_request(host, port, "GET", path)
}

fn http_request(host: &str, port: u16, method: &str, path: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect((host, port)).map_err(|e| format!("http connect: {}", e))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| format!("http timeout: {}", e))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| format!("http timeout: {}", e))?;
    let req = format!(
        "{} {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        method, path, host, port
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("http write: {}", e))?;
    let mut buf = Vec::new();
    loop {
        let mut chunk = [0u8; 1024];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(format!("http read: {}", e)),
        }
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn extract_ws_from_target(entry: &Value) -> Option<String> {
    if let Some(ws) = entry.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
        return Some(ws.to_string());
    }
    let devtools = entry.get("devtoolsFrontendUrl").and_then(|v| v.as_str())?;
    let ws_idx = devtools.find("ws=")? + 3;
    let ws_rest = &devtools[ws_idx..];
    let ws_end = ws_rest.find('&').unwrap_or(ws_rest.len());
    Some(format!("ws://{}", &ws_rest[..ws_end]))
}

#[derive(serde::Deserialize)]
struct Bounds {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn click_named(cdp: &mut CdpClient, name: &str) -> Result<(), String> {
    let bounds = find_bounds(cdp, name)?;
    let x = bounds.x + bounds.w * 0.5;
    let y = bounds.y + bounds.h * 0.5;
    tap_app(cdp, x, y)
}

fn find_bounds(cdp: &mut CdpClient, name: &str) -> Result<Bounds, String> {
    let needle = name.replace('\\', "\\\\").replace('"', "\\\"");
    let expr = format!(
        "(function(){{const target=\"{}\";const walk=(n)=>{{if(!n)return null; if((n.name && n.name.includes(target)) || (n.value && n.value.includes(target)))return n.bounds; for(const c of (n.children||[])){{const r=walk(c); if(r)return r;}} return null;}}; const b=walk(window.__a11y && window.__a11y.root); return b ? JSON.stringify(b) : \"\";}})()",
        needle
    );
    let json = cdp.eval_str(&expr)?;
    if json.is_empty() {
        let names = cdp
            .eval_str(
                "(function(){const out=[];const walk=(n)=>{if(!n)return; if(n.name)out.push(n.name); (n.children||[]).forEach(walk);}; walk(window.__a11y && window.__a11y.root); return out.join(' | ');})()",
            )
            .unwrap_or_else(|_| "names unavailable".to_string());
        let mut preview = names;
        if preview.len() > 200 {
            preview.truncate(200);
        }
        return Err(format!("a11y node not found: {} (names: {})", name, preview));
    }
    serde_json::from_str(&json).map_err(|e| format!("bounds parse error: {}", e))
}

fn tap_app(cdp: &mut CdpClient, x: f32, y: f32) -> Result<(), String> {
    let expr = format!(
        "window.__app && (window.__app.handle_pointer_down({x},{y},0,false,false,false,false), window.__app.handle_pointer_up({x},{y},0,false,false,false,false))",
        x = x,
        y = y
    );
    cdp.eval_void(&expr)
}

fn tick_app(cdp: &mut CdpClient) -> Result<(), String> {
    cdp.eval_void("window.__app && (window.__a11y = window.__app.frame(performance.now()))")
}

fn wait_for_a11y(cdp: &mut CdpClient, needle: &str, timeout: Duration) -> Result<(), String> {
    wait_for_eval_contains(
        cdp,
        "(function(){const out=[];const walk=(n)=>{if(!n)return; if(n.name)out.push(n.name); if(n.value)out.push(n.value); (n.children||[]).forEach(walk);}; walk(window.__a11y && window.__a11y.root); return out.join(' | ');})()",
        needle,
        timeout,
    )
}

fn wait_for_eval_contains(
    cdp: &mut CdpClient,
    expr: &str,
    needle: &str,
    timeout: Duration,
) -> Result<(), String> {
    let start = Instant::now();
    let mut last_err: Option<String> = None;
    while start.elapsed() < timeout {
        if let Err(err) = cdp.eval_void(
            "window.__app && (window.__a11y = window.__app.frame(performance.now()))",
        ) {
            last_err = Some(err);
        }
        if let Ok(value) = cdp.eval_str(expr) {
            if value.contains(needle) {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    let diag = cdp
        .eval_str(
            "JSON.stringify({ready:document.readyState,href:location.href,hasApp:!!window.__app,hasA11y:!!window.__a11y})",
        )
        .unwrap_or_else(|_| "diag unavailable".to_string());
    let names = cdp
        .eval_str(
            "(function(){const out=[];const walk=(n)=>{if(!n)return; if(n.name)out.push(n.name); (n.children||[]).forEach(walk);}; walk(window.__a11y && window.__a11y.root); return out.join(' | ');})()",
        )
        .unwrap_or_else(|_| "names unavailable".to_string());
    let mut names_preview = names;
    if names_preview.len() > 200 {
        names_preview.truncate(200);
    }
    if let Some(err) = last_err {
        return Err(format!(
            "timeout waiting for '{}' (diag: {}) (names: {}) (last eval err: {})",
            needle, diag, names_preview, err
        ));
    }
    Err(format!(
        "timeout waiting for '{}' (diag: {}) (names: {})",
        needle, diag, names_preview
    ))
}

struct WebSocket {
    stream: TcpStream,
}

impl WebSocket {
    fn connect(ws_url: &str) -> Result<Self, String> {
        let (host, port, path) = parse_ws_url(ws_url)?;
        let mut stream = TcpStream::connect((host.as_str(), port))
            .map_err(|e| format!("ws connect: {}", e))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| format!("ws timeout: {}", e))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| format!("ws timeout: {}", e))?;
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n\r\n",
            path, host, port, key
        );
        stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("ws handshake write: {}", e))?;
        let mut resp = [0u8; 1024];
        match stream.read(&mut resp) {
            Ok(0) => return Err("ws handshake empty response".to_string()),
            Ok(_) => {
                let text = String::from_utf8_lossy(&resp);
                if !text.contains("101") {
                    return Err("ws handshake failed".to_string());
                }
            }
            Err(e) => {
                return Err(format!("ws handshake read: {}", e));
            }
        }
        Ok(Self { stream })
    }

    fn send_text(&mut self, text: &str) -> Result<(), String> {
        let mut frame = Vec::new();
        frame.push(0x81);
        let payload = text.as_bytes();
        let len = payload.len();
        if len < 126 {
            frame.push(0x80 | len as u8);
        } else if len < 65536 {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }
        let mask = [0x12u8, 0x34, 0x56, 0x78];
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream
            .write_all(&frame)
            .map_err(|e| format!("ws send: {}", e))
    }

    fn read_text(&mut self, timeout: Duration) -> Result<Option<String>, String> {
        self.stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| format!("ws timeout: {}", e))?;
        let mut header = [0u8; 2];
        if let Err(e) = self.stream.read_exact(&mut header) {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(format!("ws read header: {}", e));
        }
        let opcode = header[0] & 0x0f;
        let masked = (header[1] & 0x80) != 0;
        let mut len = (header[1] & 0x7f) as u64;
        if len == 126 {
            let mut buf = [0u8; 2];
            self.stream
                .read_exact(&mut buf)
                .map_err(|e| format!("ws read len: {}", e))?;
            len = u16::from_be_bytes(buf) as u64;
        } else if len == 127 {
            let mut buf = [0u8; 8];
            self.stream
                .read_exact(&mut buf)
                .map_err(|e| format!("ws read len: {}", e))?;
            len = u64::from_be_bytes(buf);
        }
        let mut mask = [0u8; 4];
        if masked {
            self.stream
                .read_exact(&mut mask)
                .map_err(|e| format!("ws read mask: {}", e))?;
        }
        let mut payload = vec![0u8; len as usize];
        self.stream
            .read_exact(&mut payload)
            .map_err(|e| format!("ws read payload: {}", e))?;
        if masked {
            for i in 0..payload.len() {
                payload[i] ^= mask[i % 4];
            }
        }
        if opcode == 1 {
            let text = String::from_utf8_lossy(&payload).to_string();
            Ok(Some(text))
        } else {
            Ok(None)
        }
    }
}

fn parse_ws_url(ws_url: &str) -> Result<(String, u16, String), String> {
    if !ws_url.starts_with("ws://") {
        return Err("only ws:// supported".to_string());
    }
    let rest = ws_url.trim_start_matches("ws://");
    let mut parts = rest.splitn(2, '/');
    let host_port = parts.next().unwrap_or("");
    let path = format!("/{}", parts.next().unwrap_or(""));
    let mut host_parts = host_port.splitn(2, ':');
    let host = host_parts.next().unwrap_or("127.0.0.1").to_string();
    let port = host_parts
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(9222);
    Ok((host, port, path))
}

struct CdpClient<'a> {
    ws: &'a mut WebSocket,
    next_id: u64,
}

impl<'a> CdpClient<'a> {
    fn new(ws: &'a mut WebSocket) -> Self {
        Self { ws, next_id: 1 }
    }

    fn send(&mut self, method: &str, params: &str) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = format!(
            "{{\"id\":{},\"method\":\"{}\",\"params\":{}}}",
            id, method, params
        );
        self.ws.send_text(&msg)?;
        let _ = self.wait_for_id(id, Duration::from_secs(5))?;
        Ok(id)
    }

    fn eval_str(&mut self, expr: &str) -> Result<String, String> {
        let id = self.next_id;
        self.next_id += 1;
        let params = format!(
            "{{\"expression\":\"{}\",\"returnByValue\":true}}",
            expr.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let msg = format!(
            "{{\"id\":{},\"method\":\"Runtime.evaluate\",\"params\":{}}}",
            id, params
        );
        self.ws.send_text(&msg)?;
        let resp = self.wait_for_id(id, Duration::from_secs(2))?;
        extract_eval_value(&resp)
    }

    fn eval_void(&mut self, expr: &str) -> Result<(), String> {
        let id = self.next_id;
        self.next_id += 1;
        let params = format!(
            "{{\"expression\":\"{}\",\"returnByValue\":false}}",
            expr.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let msg = format!(
            "{{\"id\":{},\"method\":\"Runtime.evaluate\",\"params\":{}}}",
            id, params
        );
        self.ws.send_text(&msg)?;
        let resp = self.wait_for_id(id, Duration::from_secs(2))?;
        if let Some(err) = resp.get("error") {
            return Err(format!("eval error: {}", err));
        }
        if let Some(err) = resp.pointer("/result/exceptionDetails") {
            return Err(format!("eval exception: {}", err));
        }
        Ok(())
    }

    fn wait_for_id(&mut self, id: u64, timeout: Duration) -> Result<Value, String> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(msg) = self.ws.read_text(Duration::from_millis(100))? {
                if let Ok(val) = serde_json::from_str::<Value>(&msg) {
                    if val.get("id").and_then(|v| v.as_u64()) == Some(id) {
                        return Ok(val);
                    }
                }
            }
        }
        Err(format!("cdp response timeout for {}", id))
    }
}

fn extract_eval_value(resp: &Value) -> Result<String, String> {
    if let Some(value) = resp.pointer("/result/result/value") {
        if let Some(text) = value.as_str() {
            return Ok(text.to_string());
        }
        return Ok(value.to_string());
    }
    if let Some(desc) = resp.pointer("/result/result/description") {
        if let Some(text) = desc.as_str() {
            return Ok(text.to_string());
        }
        return Ok(desc.to_string());
    }
    if let Some(err) = resp.get("error") {
        return Err(format!("eval error: {}", err));
    }
    if let Some(err) = resp.pointer("/result/exceptionDetails") {
        return Err(format!("eval exception: {}", err));
    }
    Err("eval value missing".to_string())
}
