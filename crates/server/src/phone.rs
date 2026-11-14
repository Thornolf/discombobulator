use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PhoneStatus {
    pub tailscale: bool,
    pub url: Option<String>,
    pub qr_svg: Option<String>,
    pub error: Option<String>,
    pub action_url: Option<String>,
    #[serde(skip)]
    pub machine: Option<String>,
}

#[derive(Deserialize)]
struct TailscaleStatus {
    #[serde(rename = "Self")]
    me: Option<SelfNode>,
}

#[derive(Deserialize)]
struct SelfNode {
    #[serde(rename = "DNSName")]
    dns_name: Option<String>,
}

pub fn install_dir_from_reg_query(output: &str) -> Option<PathBuf> {
    output.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        if !parts.next()?.eq_ignore_ascii_case("InstallDir") {
            return None;
        }
        if !parts.next()?.starts_with("REG_") {
            return None;
        }
        let rest = line.split_once("REG_")?.1;
        let value = rest.split_once(char::is_whitespace)?.1.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

#[cfg(windows)]
fn registered_install_dir() -> Option<PathBuf> {
    let system_root = std::env::var_os("SystemRoot")?;
    let output = run(
        &PathBuf::from(system_root).join("System32").join("reg.exe"),
        &["query", r"HKLM\SOFTWARE\Tailscale IPN", "/v", "InstallDir"],
    )
    .ok()?;
    install_dir_from_reg_query(&output)
}

#[cfg(not(windows))]
fn registered_install_dir() -> Option<PathBuf> {
    None
}

fn tailscale_binary() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        let program_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        registered_install_dir()
            .map(|dir| dir.join("tailscale.exe"))
            .into_iter()
            .chain([program_files.join("Tailscale").join("tailscale.exe")])
            .collect()
    } else {
        vec![
            PathBuf::from("/usr/bin/tailscale"),
            PathBuf::from("/usr/local/bin/tailscale"),
            PathBuf::from("/opt/homebrew/bin/tailscale"),
            PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale"),
        ]
    };
    candidates.into_iter().find(|path| path.is_file())
}

const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Failure {
    message: String,
    action_url: Option<String>,
}

impl Failure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            action_url: None,
        }
    }
}

#[derive(Default)]
struct Capture {
    buffer: Arc<Mutex<Vec<u8>>>,
    reader: Option<JoinHandle<()>>,
}

impl Capture {
    fn text(&self) -> String {
        self.buffer
            .lock()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    }

    fn finish(&mut self) -> String {
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.text()
    }
}

fn collect(mut source: impl Read + Send + 'static) -> Capture {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buffer);
    let reader = std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(read) = source.read(&mut chunk) {
            if read == 0 {
                break;
            }
            if let Ok(mut bytes) = sink.lock() {
                bytes.extend_from_slice(&chunk[..read]);
            }
        }
    });
    Capture {
        buffer,
        reader: Some(reader),
    }
}

pub fn action_url(text: &str) -> Option<String> {
    let start = text.find("https://login.tailscale.com/")?;
    let url: String = text[start..]
        .chars()
        .take_while(|character| !character.is_whitespace())
        .collect();
    let url = url.trim_end_matches(['.', ',', ';', ')', '"', '\'']);
    url.chars()
        .all(|character| character.is_ascii_alphanumeric() || "/:.?=&-_%~+".contains(character))
        .then(|| url.to_owned())
}

fn failure_from(stdout: &str, stderr: &str, fallback: &str) -> Failure {
    let combined = format!("{stderr}\n{stdout}");
    let action_url = action_url(&combined);
    let message = if action_url.is_some() {
        "Tailscale needs Serve enabled for your tailnet. Open the link, approve it, then try again."
            .to_owned()
    } else if stderr.trim().is_empty() {
        fallback.to_owned()
    } else {
        stderr.trim().to_owned()
    };
    Failure {
        message,
        action_url,
    }
}

fn run(binary: &PathBuf, arguments: &[&str]) -> Result<String, Failure> {
    let mut command = Command::new(binary);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|error| Failure::new(error.to_string()))?;
    let mut stdout = child.stdout.take().map(collect).unwrap_or_default();
    let mut stderr = child.stderr.take().map(collect).unwrap_or_default();
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = stdout.finish();
                let err = stderr.finish();
                if status.success() {
                    return Ok(out);
                }
                return Err(failure_from(&out, &err, "Tailscale failed"));
            }
            Ok(None) if started.elapsed() >= COMMAND_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failure_from(
                    &stdout.text(),
                    &stderr.text(),
                    "Tailscale did not answer in time",
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(Failure::new(error.to_string())),
        }
    }
}

pub fn serves_port(serve_json: &str, port: u16) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(serve_json) else {
        return false;
    };
    let target = format!("http://127.0.0.1:{port}");
    value
        .get("Web")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|sites| sites.values())
        .filter_map(|site| site.get("Handlers").and_then(serde_json::Value::as_object))
        .flat_map(|handlers| handlers.values())
        .filter_map(|handler| handler.get("Proxy").and_then(serde_json::Value::as_str))
        .any(|proxy| proxy == target)
}

pub fn foreign_https_handler(serve_json: &str, port: u16) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(serve_json) else {
        return false;
    };
    let target = format!("http://127.0.0.1:{port}");
    let raw_forward = value
        .get("TCP")
        .and_then(|tcp| tcp.get("443"))
        .is_some_and(|entry| entry.get("TCPForward").is_some());
    let other_web_handler = value
        .get("Web")
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|sites| sites.iter())
        .filter(|(site, _)| site.ends_with(":443"))
        .filter_map(|(_, site)| site.get("Handlers").and_then(serde_json::Value::as_object))
        .flat_map(|handlers| handlers.values())
        .any(|handler| handler.get("Proxy").and_then(serde_json::Value::as_str) != Some(&target));
    raw_forward || other_web_handler
}

pub fn dns_name(status_json: &str) -> Option<String> {
    let status: TailscaleStatus = serde_json::from_str(status_json).ok()?;
    let name = status.me?.dns_name?;
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    crate::guard::is_allowed_host(&name)
        .then_some(name)
        .filter(|name| name.ends_with(".ts.net"))
}

pub fn qr_svg(text: &str) -> Option<String> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    Some(
        code.render::<qrcode::render::svg::Color<'_>>()
            .min_dimensions(220, 220)
            .quiet_zone(true)
            .build(),
    )
}

fn status_with(url: Option<String>, failure: Option<Failure>) -> PhoneStatus {
    let (error, action_url) = failure.map_or((None, None), |failure| {
        (Some(failure.message), failure.action_url)
    });
    PhoneStatus {
        tailscale: true,
        qr_svg: url.as_deref().and_then(qr_svg),
        url,
        error,
        action_url,
        machine: None,
    }
}

fn missing() -> PhoneStatus {
    PhoneStatus {
        tailscale: false,
        url: None,
        qr_svg: None,
        error: Some("Tailscale is not installed on this PC".into()),
        action_url: None,
        machine: None,
    }
}

pub fn machine_name() -> Option<String> {
    let binary = tailscale_binary()?;
    dns_name(&run(&binary, &["status", "--json"]).ok()?)
}

pub fn status(port: u16) -> PhoneStatus {
    let Some(binary) = tailscale_binary() else {
        return missing();
    };
    let name = match run(&binary, &["status", "--json"]) {
        Ok(json) => dns_name(&json),
        Err(failure) => return status_with(None, Some(failure)),
    };
    let serving = run(&binary, &["serve", "status", "--json"])
        .map(|json| serves_port(&json, port))
        .unwrap_or(false);
    let mut status = match &name {
        Some(name) if serving => status_with(Some(format!("https://{name}/")), None),
        Some(_) => status_with(None, None),
        None => status_with(None, Some(Failure::new("Tailscale is not connected"))),
    };
    status.machine = name;
    status
}

pub fn enable(port: u16) -> PhoneStatus {
    let Some(binary) = tailscale_binary() else {
        return missing();
    };
    let target = format!("http://127.0.0.1:{port}");
    let occupied = run(&binary, &["serve", "status", "--json"])
        .is_ok_and(|json| foreign_https_handler(&json, port));
    if occupied {
        return status_with(
            None,
            Some(Failure::new(
                "Tailscale already serves something else on HTTPS port 443 of this PC. \
                 Discombobulator will not replace it. Remove that handler \
                 (tailscale serve --https=443 off) if you no longer need it, then try again.",
            )),
        );
    }
    if let Err(failure) = run(&binary, &["serve", "--bg", "--https=443", &target]) {
        return status_with(None, Some(failure));
    }
    status(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_machine_name_from_tailscale_status() {
        let json = r#"{"Self":{"DNSName":"Desk.tail1234.ts.net.","HostName":"desk"}}"#;
        assert_eq!(dns_name(json), Some("desk.tail1234.ts.net".into()));
    }

    #[test]
    fn ignores_names_outside_the_tailnet() {
        assert_eq!(dns_name(r#"{"Self":{"DNSName":"evil.com."}}"#), None);
        assert_eq!(dns_name(r#"{"Self":{}}"#), None);
        assert_eq!(dns_name("not json"), None);
    }

    #[test]
    fn detects_the_app_in_serve_status() {
        let json = r#"{"TCP":{"443":{"HTTPS":true}},"Web":{"desk.tail1234.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:47820"}}}}}"#;
        assert!(serves_port(json, 47820));
        assert!(!serves_port(json, 4782));
        assert!(!serves_port(json, 47821));
    }

    #[test]
    fn treats_unknown_serve_status_as_not_serving() {
        let other = r#"{"Web":{"x:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:478200"}}}}}"#;
        assert!(!serves_port(other, 47820));
        assert!(!serves_port("{}", 47820));
        assert!(!serves_port("", 47820));
        assert!(!serves_port(r#"{"Web":[1,2]}"#, 47820));
        assert!(!serves_port("127.0.0.1:47820", 47820));
    }

    #[test]
    fn spots_another_handler_on_port_443() {
        let ours = r#"{"TCP":{"443":{"HTTPS":true}},"Web":{"desk.tail1234.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:47820"}}}}}"#;
        assert!(!foreign_https_handler(ours, 47820));
        let other = r#"{"TCP":{"443":{"HTTPS":true}},"Web":{"desk.tail1234.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:3000"}}}}}"#;
        assert!(foreign_https_handler(other, 47820));
        let extra_path = r#"{"Web":{"desk.tail1234.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:47820"},"/files":{"Path":"/srv"}}}}}"#;
        assert!(foreign_https_handler(extra_path, 47820));
        let raw = r#"{"TCP":{"443":{"TCPForward":"127.0.0.1:22"}}}"#;
        assert!(foreign_https_handler(raw, 47820));
        let other_port = r#"{"Web":{"desk.tail1234.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:3000"}}}}}"#;
        assert!(!foreign_https_handler(other_port, 47820));
        assert!(!foreign_https_handler("{}", 47820));
        assert!(!foreign_https_handler("", 47820));
    }

    #[test]
    fn reads_the_install_dir_from_reg_query_output() {
        let output = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Tailscale IPN\r\n    InstallDir    REG_SZ    D:\\Apps\\Tailscale\r\n\r\n";
        assert_eq!(
            install_dir_from_reg_query(output),
            Some(PathBuf::from(r"D:\Apps\Tailscale"))
        );
        assert_eq!(install_dir_from_reg_query("ERROR: not found"), None);
        assert_eq!(
            install_dir_from_reg_query("    InstallDir    REG_SZ    "),
            None
        );
    }

    #[test]
    fn extracts_the_enable_serve_link() {
        let output = "Serve is not enabled on your tailnet.\nTo enable, visit:\n\n         https://login.tailscale.com/f/serve?node=nAbC123\n";
        assert_eq!(
            action_url(output),
            Some("https://login.tailscale.com/f/serve?node=nAbC123".into())
        );
        let failure = failure_from(output, "", "Tailscale failed");
        assert_eq!(
            failure.action_url.as_deref(),
            Some("https://login.tailscale.com/f/serve?node=nAbC123")
        );
        assert_eq!(
            action_url("visit https://evil.com/login.tailscale.com/"),
            None
        );
        assert_eq!(action_url("no link here"), None);
    }

    #[test]
    fn renders_a_qr_code() {
        let svg = qr_svg("https://desk.tail1234.ts.net/").unwrap_or_default();
        assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
    }
}
