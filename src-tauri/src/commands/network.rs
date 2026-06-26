use hickory_resolver::config::{ConnectionConfig, NameServerConfig, ResolverConfig, ResolverOpts};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::RecordType;
use hickory_resolver::{Resolver, TokioResolver};
use reqwest;
use serde::Serialize;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use lazy_static::lazy_static;

// 1. 全局静态变量，用于存放前端传来的 Self Proxy 端口拼接后的字符串
lazy_static! {
    static ref SELF_PROXY: Mutex<String> = Mutex::new(String::new());
}

// 2. 提供给前端调用的命令：更新全局代理变量
#[tauri::command]
pub fn set_self_proxy(proxy: String) {
    let mut p = SELF_PROXY.lock().unwrap();
    *p = proxy;
}

fn get_system_proxy() -> Option<String> {
// --- Windows 逻辑 ---
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let settings = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
            .ok()?;

        let enabled: u32 = settings.get_value("ProxyEnable").ok()?;
        if enabled == 0 { return None; }

        let server: String = settings.get_value("ProxyServer").ok()?;
        if server.is_empty() { return None; }

        let addr = if server.contains('=') {
            server.split(';')
                .find(|s| s.starts_with("http="))
                .map(|s| s.trim_start_matches("http=").to_string())
                .unwrap_or_else(|| server.split(';').next().unwrap_or("").to_string())
        } else {
            server
        };

        if addr.is_empty() { return None; }
        return Some(if addr.contains("://") { addr } else { format!("http://{}", addr) });
    }

    // --- macOS 逻辑 ---
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let out = Command::new("scutil").args(["--proxy"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut enabled = false;
        let mut host = String::new();
        let mut port = String::new();

        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("HTTPEnable") { enabled = line.ends_with(": 1"); }
            else if line.starts_with("HTTPProxy") { host = line.split(':').nth(1)?.trim().to_string(); }
            else if line.starts_with("HTTPPort") { port = line.split(':').nth(1)?.trim().to_string(); }
        }
        if enabled && !host.is_empty() { Some(format!("http://{}:{}", host, port)) } else { None }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    None
}

fn build_client(timeout: Option<Duration>) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder();

    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }

    // 优先级 1: 检查 Self Proxy (前端传来的 socks5h://127.0.0.1:端口)
    let self_p = SELF_PROXY.lock().unwrap().clone();
    
    let proxy_to_use = if !self_p.is_empty() {
        Some(self_p)
    } else {
        // 优先级 2: 回退到系统代理
        get_system_proxy()
    };

    match proxy_to_use.and_then(|url| reqwest::Proxy::all(&url).ok()) {
        Some(proxy) => builder.proxy(proxy),
        None => builder.no_proxy(),
    }
    .build()
}

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    let client = build_client(None).map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    resp.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn http_ping(url: String, count: u32) -> Result<f64, String> {
    let client = build_client(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;

    let mut total = 0.0;
    let mut success = 0u32;

    for _ in 0..count {
        let start = Instant::now();
        if client.head(&url).send().await.is_ok() {
            total += start.elapsed().as_secs_f64() * 1000.0;
            success += 1;
        }
    }

    if success == 0 {
        return Err("timeout".to_string());
    }
    Ok(total / success as f64)
}

#[derive(Serialize)]
pub struct DnsRecord {
    record_type: String,
    name: String,
    ttl: u32,
    value: String,
    ip: Option<String>,
}

#[derive(Serialize)]
pub struct DnsQueryResult {
    source: String,
    server: Option<String>,
    records: Vec<DnsRecord>,
}

fn parse_record_type(record_type: &str) -> Result<RecordType, String> {
    let normalized = record_type.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return Err("DNS record type is empty".to_string());
    }
    RecordType::from_str(&normalized)
        .map_err(|_| format!("unsupported DNS record type: {}", normalized))
}

fn parse_dns_server(server: &str) -> Result<(IpAddr, u16), String> {
    let trimmed = server.trim();
    if trimmed.is_empty() {
        return Err("DNS server is empty".to_string());
    }

    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Ok((ip, 53));
    }

    if let Some((host, port_text)) = trimmed.rsplit_once(':') {
        let ip = host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .map_err(|_| "DNS server must be an IP address".to_string())?;
        let port = port_text
            .parse::<u16>()
            .map_err(|_| "DNS server port is invalid".to_string())?;
        return Ok((ip, port));
    }

    Err("DNS server must be an IP address".to_string())
}

fn display_dns_value(data: &hickory_resolver::proto::rr::RData) -> String {
    match data {
        hickory_resolver::proto::rr::RData::TXT(txt) => txt
            .txt_data
            .iter()
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect::<Vec<_>>()
            .join(""),
        _ => data.to_string(),
    }
}

#[tauri::command]
pub async fn dns_query(
    domain: String,
    record_type: String,
    server: Option<String>,
) -> Result<DnsQueryResult, String> {
    let domain = domain.trim().trim_end_matches('.').to_string();
    if domain.is_empty() {
        return Err("domain is empty".to_string());
    }

    let record_type = parse_record_type(&record_type)?;
    let server = server.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let resolver = if let Some(server_text) = server.as_deref() {
        let (ip, port) = parse_dns_server(server_text)?;
        let mut udp = ConnectionConfig::udp();
        udp.port = port;
        let mut tcp = ConnectionConfig::tcp();
        tcp.port = port;
        let name_server = NameServerConfig::new(ip, true, vec![udp, tcp]);
        let config = ResolverConfig::from_parts(None, vec![], vec![name_server]);
        Resolver::builder_with_config(config, TokioRuntimeProvider::default())
            .with_options(ResolverOpts::default())
            .build()
            .map_err(|e| e.to_string())?
    } else {
        TokioResolver::builder_tokio()
            .map_err(|e| e.to_string())?
            .with_options(ResolverOpts::default())
            .build()
            .map_err(|e| e.to_string())?
    };

    let lookup = resolver
        .lookup(format!("{}.", domain), record_type)
        .await
        .map_err(|e| e.to_string())?;

    let records = lookup
        .answers()
        .iter()
        .filter_map(|record| {
            let data = &record.data;
            Some(DnsRecord {
                record_type: data.record_type().to_string(),
                name: record.name.to_string(),
                ttl: record.ttl,
                value: display_dns_value(data),
                ip: data.ip_addr().map(|ip| ip.to_string()),
            })
        })
        .collect::<Vec<_>>();

    Ok(DnsQueryResult {
        source: if server.is_some() {
            "custom".to_string()
        } else {
            "system".to_string()
        },
        server,
        records,
    })
}