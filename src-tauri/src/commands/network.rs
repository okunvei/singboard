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

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    // 3. 在发起请求前，先看看盒子里有没有代理地址
    let proxy_str = SELF_PROXY.lock().unwrap().clone();
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        // 伪装成 Chrome 浏览器，防止被订阅服务器拒绝
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

    // 如果填了代理，就给请求客户端装上代理
    if !proxy_str.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_str) {
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
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(5));

    // 测速同样应用代理，否则测出来的延迟是不准的
    if !proxy_str.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_str) {
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
