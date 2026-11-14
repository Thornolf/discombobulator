use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PhoneStatus {
    pub tailscale: bool,
    pub url: Option<String>,
    pub qr_svg: Option<String>,
    pub error: Option<String>,
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

fn tailscale_binary() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        let program_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        vec![program_files.join("Tailscale").join("tailscale.exe")]
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

fn run(binary: &PathBuf, arguments: &[&str]) -> Result<String, String> {
    let mut command = Command::new(binary);
    command.args(arguments);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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

fn status_with(url: Option<String>, error: Option<String>) -> PhoneStatus {
    PhoneStatus {
        tailscale: true,
        qr_svg: url.as_deref().and_then(qr_svg),
        url,
        error,
    }
}

fn missing() -> PhoneStatus {
    PhoneStatus {
        tailscale: false,
        url: None,
        qr_svg: None,
        error: Some("Tailscale is not installed on this PC".into()),
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
        Err(error) => return status_with(None, Some(error)),
    };
    let serving = run(&binary, &["serve", "status"])
        .map(|text| text.contains(&format!("127.0.0.1:{port}")))
        .unwrap_or(false);
    match name {
        Some(name) if serving => status_with(Some(format!("https://{name}/")), None),
        Some(_) => status_with(None, None),
        None => status_with(None, Some("Tailscale is not connected".into())),
    }
}

pub fn enable(port: u16) -> PhoneStatus {
    let Some(binary) = tailscale_binary() else {
        return missing();
    };
    let target = format!("http://127.0.0.1:{port}");
    if let Err(error) = run(&binary, &["serve", "--bg", "--https=443", &target]) {
        return status_with(None, Some(error));
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
    fn renders_a_qr_code() {
        let svg = qr_svg("https://desk.tail1234.ts.net/").unwrap_or_default();
        assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
    }
}
