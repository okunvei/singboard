use reqwest;
use std::time::{Duration, Instant};
use std::sync::Mutex;
use lazy_static::lazy_static; // 注意：如果报错，请看下方说明

// 1. 创建一个全局的“代理盒子”，让所有网络函数都能读取
lazy_static! {
    static ref SELF_PROXY: Mutex<String> = Mutex::new(String::new());
}

// 2. 接收前端发来的代理设置并存入盒子
#[tauri::command]
pub fn set_self_proxy(proxy: String) {
    let mut p = SELF_PROXY.lock().unwrap();
    *p = proxy;
    println!("网络模块已收到代理更新: {}", *p);
}

/// 从 macOS scutil 读取当前系统代理设置
/// 返回形如 "http://127.0.0.1:7890" 的字符串，没有代理则返回空字符串
#[cfg(target_os = "macos")]
fn get_macos_system_proxy() -> String {
    use std::process::Command;
    let out = match Command::new("scutil").args(["--proxy"]).output() {
        Ok(o) => o,
        Err(_) => return String::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);

    // 解析 HTTPEnable / HTTPProxy / HTTPPort
    let mut http_enabled = false;
    let mut http_host = String::new();
    let mut http_port = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("HTTPEnable") {
            http_enabled = line.ends_with(": 1");
        } else if line.starts_with("HTTPProxy") {
            http_host = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
        } else if line.starts_with("HTTPPort") {
            http_port = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
        }
    }

    if http_enabled && !http_host.is_empty() && !http_port.is_empty() {
        format!("http://{}:{}", http_host, http_port)
    } else {
        String::new()
    }
}

#[cfg(not(target_os = "macos"))]
fn get_macos_system_proxy() -> String {
    String::new()
}

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    // 3. 在发起请求前，先看看盒子里有没有代理地址
    let proxy_str = SELF_PROXY.lock().unwrap().clone();

    // 自身代理优先，为空时回退到 macOS 系统代理
    let effective_proxy = if !proxy_str.is_empty() {
        proxy_str
    } else {
        get_macos_system_proxy()
    };

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        // 伪装成 Chrome 浏览器，防止被订阅服务器拒绝
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

    // 如果填了代理，就给请求客户端装上代理
    if !effective_proxy.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&effective_proxy) {
            builder = builder.proxy(proxy);
        }
    }

    let client = builder.build().map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    resp.text().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn http_ping(url: String, count: u32) -> Result<f64, String> {
    let proxy_str = SELF_PROXY.lock().unwrap().clone();

    // 自身代理优先，为空时回退到 macOS 系统代理
    let effective_proxy = if !proxy_str.is_empty() {
        proxy_str
    } else {
        get_macos_system_proxy()
    };

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(5));

    // 测速同样应用代理，否则测出来的延迟是不准的
    if !effective_proxy.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&effective_proxy) {
            builder = builder.proxy(proxy);
        }
    }

    let client = builder.build().map_err(|e| e.to_string())?;
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

/// 读取运行配置，检测是否有 mixed 入站且 set_system_proxy=true
#[tauri::command]
pub async fn check_system_proxy_inbound(config_path: String) -> Result<bool, String> {
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| format!("读取配置失败: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析配置失败: {}", e))?;

    let inbounds = match json.get("inbounds").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Ok(false),
    };

    for inbound in inbounds {
        let is_mixed = inbound.get("type").and_then(|v| v.as_str()) == Some("mixed");
        let set_proxy = inbound
            .get("set_system_proxy")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_mixed && set_proxy {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 清除 macOS 系统代理（HTTP、HTTPS、SOCKS）
/// 遍历所有网络服务，逐一关闭代理
#[tauri::command]
pub async fn clear_macos_system_proxy() -> Result<(), String> {
    // 获取所有网络服务列表
    let services_out = tokio::process::Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
        .await
        .map_err(|e| format!("获取网络服务列表失败: {}", e))?;

    let services_text = String::from_utf8_lossy(&services_out.stdout);
    // 第一行是提示语，跳过
    let services: Vec<&str> = services_text
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('*'))
        .collect();

    for svc in &services {
        // 关闭 HTTP 代理
        let _ = tokio::process::Command::new("networksetup")
            .args(["-setwebproxystate", svc, "off"])
            .output()
            .await;
        // 关闭 HTTPS 代理
        let _ = tokio::process::Command::new("networksetup")
            .args(["-setsecurewebproxystate", svc, "off"])
            .output()
            .await;
        // 关闭 SOCKS 代理
        let _ = tokio::process::Command::new("networksetup")
            .args(["-setsocksfirewallproxystate", svc, "off"])
            .output()
            .await;
    }

    Ok(())
}