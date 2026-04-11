//! singboard-helper — 以 root 身份常驻运行的特权 Helper
//!
//! 功能：
//! 1. 通过 Unix Socket 接受主 App 的 JSON 指令，执行 launchctl 操作
//! 2. 监听 SCDynamicStore 网络状态变化，自动维持 DNS 设置
//!    - 启动核心后：解析 TUN 入站的 address，将所有网卡 DNS 设为 TUN 网关 IP
//!    - 网络切换时：自动重新应用 DNS（防止 wifi 切换/合盖后失效）
//!    - 停止核心后：清除所有网卡的手动 DNS，恢复自动，并刷新 DNS 缓存

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

use std::ffi::{c_void, CStr, CString};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

const SOCKET_PATH: &str = "/var/run/singboard-helper.sock";
const PLIST_DIR: &str = "/Library/LaunchDaemons";

// ── SCDynamicStore / SystemConfiguration FFI ────────────────────────────────

#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    fn SCDynamicStoreCreate(
        allocator: *const c_void,
        name: CFStringRef,
        callout: Option<SCDynamicStoreCallBack>,
        context: *mut SCDynamicStoreContext,
    ) -> SCDynamicStoreRef;

    fn SCDynamicStoreSetNotificationKeys(
        store: SCDynamicStoreRef,
        keys: CFArrayRef,
        patterns: CFArrayRef,
    ) -> bool;

    fn SCDynamicStoreCreateRunLoopSource(
        allocator: *const c_void,
        store: SCDynamicStoreRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        cStr: *const i8,
        encoding: u32,
    ) -> CFStringRef;
    fn CFArrayCreate(
        allocator: *const c_void,
        values: *const *const c_void,
        numValues: CFIndex,
        callbacks: *const c_void,
    ) -> CFArrayRef;
    fn CFRelease(cf: *const c_void);
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    static kCFRunLoopDefaultMode: CFStringRef;
    static kCFTypeArrayCallBacks: c_void;
}

type CFStringRef = *const c_void;
type CFArrayRef = *const c_void;
type CFIndex = isize;
type CFRunLoopRef = *const c_void;
type CFRunLoopSourceRef = *const c_void;
type SCDynamicStoreRef = *const c_void;
type SCDynamicStoreCallBack = unsafe extern "C" fn(
    store: SCDynamicStoreRef,
    changedKeys: CFArrayRef,
    info: *mut c_void,
);

#[repr(C)]
struct SCDynamicStoreContext {
    version: CFIndex,
    info: *mut c_void,
    retain: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
    release: Option<unsafe extern "C" fn(*const c_void)>,
    copyDescription: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
}

const kCFStringEncodingUTF8: u32 = 0x08000100;

unsafe fn cfstr(s: &str) -> CFStringRef {
    let c = CString::new(s).unwrap();
    unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), kCFStringEncodingUTF8) }
}

// ── 全局 DNS 状态 ─────────────────────────────────────────────────────────────

/// 当前需要维持的 DNS 服务器列表，None 表示不维持（核心未运行）
static DNS_TARGETS: Mutex<Option<Vec<String>>> = Mutex::new(None);

fn set_dns_targets(ips: Option<Vec<String>>) {
    let mut t = DNS_TARGETS.lock().unwrap();
    *t = ips;
}

fn get_dns_targets() -> Option<Vec<String>> {
    DNS_TARGETS.lock().unwrap().clone()
}

// ── TUN 配置解析 ─────────────────────────────────────────────────────────────

/// TUN 网关解析结果，包含 IPv4 和可选的 IPv6 网关
struct TunGateways {
    ipv4: Option<String>,
    ipv6: Option<String>,
}

/// 从 CIDR 字符串提取网关 IP（取 "/" 前的 IP 部分）
/// 验证必须是合法的 IPv4 或 IPv6 地址
fn extract_gateway_ip(cidr: &str) -> Option<(String, bool)> {
    let ip_part = cidr.split('/').next()?.trim();
    // 先尝试 IPv4
    if Ipv4Addr::from_str(ip_part).is_ok() {
        return Some((ip_part.to_string(), false));
    }
    // 再尝试 IPv6
    if Ipv6Addr::from_str(ip_part).is_ok() {
        return Some((ip_part.to_string(), true));
    }
    None
}

/// 从 sing-box running-config 里解析 TUN 入站的网关 IP
/// address 字段可能是字符串（单个）或数组（多个）
/// 返回 IPv4 网关和可选的 IPv6 网关
fn parse_tun_gateways(config_path: &str) -> Option<TunGateways> {
    let content = fs::read_to_string(config_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    let inbounds = json.get("inbounds")?.as_array()?;
    for inbound in inbounds {
        if inbound.get("type")?.as_str()? != "tun" {
            continue;
        }

        let mut ipv4_gw: Option<String> = None;
        let mut ipv6_gw: Option<String> = None;

        // address 字段：字符串或数组两种格式
        let addresses: Vec<String> = if let Some(s) = inbound.get("address").and_then(|v| v.as_str()) {
            vec![s.to_string()]
        } else if let Some(arr) = inbound.get("address").and_then(|v| v.as_array()) {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        } else {
            continue;
        };

        for addr in &addresses {
            if let Some((ip, is_v6)) = extract_gateway_ip(addr) {
                if is_v6 && ipv6_gw.is_none() {
                    ipv6_gw = Some(ip);
                } else if !is_v6 && ipv4_gw.is_none() {
                    ipv4_gw = Some(ip);
                }
            }
        }

        // 找到 TUN 入站就返回（只处理第一个 TUN 入站）
        if ipv4_gw.is_some() || ipv6_gw.is_some() {
            return Some(TunGateways { ipv4: ipv4_gw, ipv6: ipv6_gw });
        }
    }

    None
}

// ── DNS 设置（通过 networksetup） ────────────────────────────────────────────

/// 获取所有活跃的网络服务名称
fn get_network_services() -> Vec<String> {
    let out = match Command::new("networksetup")
        .args(["-listallnetworkservices"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('*'))
        .map(|l| l.to_string())
        .collect()
}

/// 刷新 macOS DNS 缓存
fn flush_dns_cache() {
    let _ = Command::new("dscacheutil").args(["-flushcache"]).output();
    let _ = Command::new("killall").args(["-HUP", "mDNSResponder"]).output();
    eprintln!("[helper] DNS 缓存已刷新");
}

/// 将所有网络服务的 DNS 设为指定的 IP 列表（支持同时设 IPv4 + IPv6）
fn apply_dns(ips: &[String]) {
    let services = get_network_services();
    let mut args: Vec<&str> = vec!["-setdnsservers"];
    let ip_strs: Vec<&str> = ips.iter().map(|s| s.as_str()).collect();

    for svc in &services {
        let mut cmd_args = vec!["-setdnsservers", svc.as_str()];
        cmd_args.extend(ip_strs.iter());
        let _ = Command::new("networksetup").args(&cmd_args).output();
        eprintln!("[helper] DNS 已设置: {} → {}", svc, ips.join(", "));
    }
    drop(args); // suppress unused warning

    flush_dns_cache();
}

/// 清除所有网络服务的手动 DNS（恢复自动），并刷新缓存
fn clear_dns() {
    for svc in get_network_services() {
        let _ = Command::new("networksetup")
            .args(["-setdnsservers", &svc, "Empty"])
            .output();
        eprintln!("[helper] DNS 已清除: {}", svc);
    }
    flush_dns_cache();
}

/// 根据当前 DNS_TARGETS 决定是应用还是跳过
fn reapply_dns() {
    match get_dns_targets() {
        Some(ips) if !ips.is_empty() => apply_dns(&ips),
        _ => {} // 核心未运行，不动 DNS
    }
}

// ── SCDynamicStore 网络监听 ──────────────────────────────────────────────────

unsafe extern "C" fn network_changed_callback(
    _store: SCDynamicStoreRef,
    _changed_keys: CFArrayRef,
    _info: *mut c_void,
) {
    eprintln!("[helper] 检测到网络变化，重新应用 DNS...");
    thread::sleep(Duration::from_millis(1500));
    reapply_dns();
}

fn start_network_watcher() {
    thread::spawn(|| {
        unsafe {
            let name = cfstr("singboard-helper-dns-watcher");

            let mut ctx = SCDynamicStoreContext {
                version: 0,
                info: std::ptr::null_mut(),
                retain: None,
                release: None,
                copyDescription: None,
            };

            let store = SCDynamicStoreCreate(
                std::ptr::null(),
                name,
                Some(network_changed_callback),
                &mut ctx,
            );
            CFRelease(name as *const c_void);

            if store.is_null() {
                eprintln!("[helper] SCDynamicStore 创建失败");
                return;
            }

            let pattern1 = cfstr("State:/Network/Interface/.*/IPv4");
            let pattern2 = cfstr("State:/Network/Global/IPv4");
            let patterns_arr = [pattern1, pattern2];
            let patterns = CFArrayCreate(
                std::ptr::null(),
                patterns_arr.as_ptr() as *const *const c_void,
                2,
                &kCFTypeArrayCallBacks as *const c_void,
            );

            SCDynamicStoreSetNotificationKeys(store, std::ptr::null(), patterns);
            CFRelease(patterns);
            CFRelease(pattern1 as *const c_void);
            CFRelease(pattern2 as *const c_void);

            let source = SCDynamicStoreCreateRunLoopSource(std::ptr::null(), store, 0);
            if source.is_null() {
                eprintln!("[helper] RunLoopSource 创建失败");
                CFRelease(store);
                return;
            }

            let runloop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(runloop, source, kCFRunLoopDefaultMode);
            CFRelease(source);

            eprintln!("[helper] 网络监听已启动");
            CFRunLoopRun();

            CFRelease(store);
        }
    });
}

// ── launchctl 辅助 ───────────────────────────────────────────────────────────

fn launchctl(args: &[&str]) -> (bool, String) {
    match Command::new("launchctl").args(args).output() {
        Ok(out) => {
            let success = out.status.success();
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            (success, text.trim().to_string())
        }
        Err(e) => (false, e.to_string()),
    }
}

fn plist_label(service_name: &str) -> String {
    format!("com.singboard.{}", service_name)
}

fn plist_path(service_name: &str) -> PathBuf {
    PathBuf::from(PLIST_DIR).join(format!("{}.plist", plist_label(service_name)))
}

// ── 服务操作 ─────────────────────────────────────────────────────────────────

fn cmd_ping() -> serde_json::Value {
    serde_json::json!({"ok": true, "pong": true})
}

fn cmd_status(service_name: &str) -> serde_json::Value {
    let plist = plist_path(service_name);
    if !plist.exists() {
        return serde_json::json!({"ok": true, "state": "not_installed", "pid": null});
    }

    let label = plist_label(service_name);
    let (_, text) = launchctl(&["print", &format!("system/{}", label)]);

    let pid: Option<u32> = text.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with("pid =") {
            line.split('=').nth(1).and_then(|v| v.trim().parse().ok())
        } else {
            None
        }
    });

    let state = if pid.is_some() { "running" } else { "stopped" };
    serde_json::json!({"ok": true, "state": state, "pid": pid})
}

fn cmd_install(
    service_name: &str,
    exe_path: &str,
    singbox_path: &str,
    config_path: &str,
    working_dir: &str,
    error_log: &str,
) -> serde_json::Value {
    let label = plist_label(service_name);
    let dest = plist_path(service_name);

    let run_dir = if !working_dir.trim().is_empty() {
        working_dir.trim().to_string()
    } else if let Some(p) = Path::new(config_path).parent() {
        if p.as_os_str().is_empty() { ".".into() } else { p.to_string_lossy().into() }
    } else {
        ".".into()
    };

    let plist_content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\"\n\
  \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
    <key>Label</key>\n\
    <string>{label}</string>\n\
    <key>ProgramArguments</key>\n\
    <array>\n\
        <string>{exe}</string>\n\
        <string>service</string>\n\
        <string>{service_name}</string>\n\
    </array>\n\
    <key>WorkingDirectory</key>\n\
    <string>{run_dir}</string>\n\
    <key>RunAtLoad</key>\n\
    <false/>\n\
    <key>KeepAlive</key>\n\
    <false/>\n\
    <key>StandardErrorPath</key>\n\
    <string>{error_log}</string>\n\
    <key>EnvironmentVariables</key>\n\
    <dict>\n\
        <key>SINGBOARD_SINGBOX_PATH</key>\n\
        <string>{singbox_path}</string>\n\
        <key>SINGBOARD_CONFIG_PATH</key>\n\
        <string>{config_path}</string>\n\
        <key>SINGBOARD_WORKING_DIR</key>\n\
        <string>{run_dir}</string>\n\
    </dict>\n\
</dict>\n\
</plist>\n",
        label = label, exe = exe_path, service_name = service_name,
        run_dir = run_dir, error_log = error_log,
        singbox_path = singbox_path, config_path = config_path,
    );

    if let Err(e) = fs::write(&dest, &plist_content) {
        return serde_json::json!({"ok": false, "error": format!("写入 plist 失败: {}", e)});
    }

    let _ = Command::new("chown").args(["root:wheel", &dest.to_string_lossy()]).output();
    let _ = Command::new("chmod").args(["644", &dest.to_string_lossy()]).output();

    launchctl(&["bootout", &format!("system/{}", label)]);
    let (ok, msg) = launchctl(&["bootstrap", "system", &dest.to_string_lossy()]);
    if !ok {
        eprintln!("[helper] bootstrap 警告: {}", msg);
    }

    serde_json::json!({"ok": true})
}

fn cmd_uninstall(service_name: &str) -> serde_json::Value {
    cmd_stop(service_name);

    let label = plist_label(service_name);
    let plist = plist_path(service_name);

    launchctl(&["bootout", &format!("system/{}", label)]);

    if plist.exists() {
        if let Err(e) = fs::remove_file(&plist) {
            return serde_json::json!({"ok": false, "error": format!("删除 plist 失败: {}", e)});
        }
    }

    serde_json::json!({"ok": true})
}

fn cmd_start(service_name: &str, config_path: &str) -> serde_json::Value {
    let plist = plist_path(service_name);
    if !plist.exists() {
        return serde_json::json!({"ok": false, "error": "service_not_found"});
    }

    let label = plist_label(service_name);

    launchctl(&["bootstrap", "system", &plist.to_string_lossy()]);

    let (ok, msg) = launchctl(&["kickstart", "-k", &format!("system/{}", label)]);
    if !ok {
        return serde_json::json!({"ok": false, "error": format!("启动失败: {}", msg.trim())});
    }

    // 等待最多 5 秒确认 running
    let mut started = false;
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(250));
        let status = cmd_status(service_name);
        match status["state"].as_str().unwrap_or("") {
            "running" => { started = true; break; }
            "stopped" => {
                return serde_json::json!({
                    "ok": false,
                    "error": "服务启动后立即退出，可能是配置文件有误，请检查配置"
                });
            }
            _ => continue,
        }
    }

    if !started {
        return serde_json::json!({"ok": true});
    }

    // 等待 TUN 接口真正就绪后再设置 DNS
    if !config_path.is_empty() {
        thread::sleep(Duration::from_millis(2000));

        match parse_tun_gateways(config_path) {
            Some(gws) => {
                // 按需组装 DNS 列表：IPv4 必有，IPv6 可选
                let mut dns_ips: Vec<String> = Vec::new();
                if let Some(v4) = &gws.ipv4 {
                    dns_ips.push(v4.clone());
                    eprintln!("[helper] TUN IPv4 网关: {}", v4);
                }
                if let Some(v6) = &gws.ipv6 {
                    dns_ips.push(v6.clone());
                    eprintln!("[helper] TUN IPv6 网关: {}", v6);
                }

                if !dns_ips.is_empty() {
                    eprintln!("[helper] 设置 DNS: {:?}", dns_ips);
                    set_dns_targets(Some(dns_ips.clone()));
                    apply_dns(&dns_ips);
                }
            }
            None => {
                eprintln!("[helper] 未检测到 TUN 入站，不修改 DNS");
            }
        }
    }

    serde_json::json!({"ok": true})
}

fn cmd_stop(service_name: &str) -> serde_json::Value {
    let label = plist_label(service_name);
    launchctl(&["kill", "TERM", &format!("system/{}", label)]);

    for _ in 0..30 {
        thread::sleep(Duration::from_millis(500));
        let status = cmd_status(service_name);
        if status["state"].as_str().unwrap_or("") == "stopped" {
            break;
        }
    }

    // 停止后清除 DNS 目标并恢复自动 DNS
    set_dns_targets(None);
    clear_dns();

    serde_json::json!({"ok": true})
}

fn cmd_clear_proxy() -> serde_json::Value {
    let services = get_network_services();
    for svc in &services {
        let _ = Command::new("networksetup").args(["-setwebproxystate", svc, "off"]).output();
        let _ = Command::new("networksetup").args(["-setsecurewebproxystate", svc, "off"]).output();
        let _ = Command::new("networksetup").args(["-setsocksfirewallproxystate", svc, "off"]).output();
    }
    serde_json::json!({"ok": true})
}

// ── 指令分发 ─────────────────────────────────────────────────────────────────

fn dispatch(req: &serde_json::Value) -> serde_json::Value {
    let cmd = req["cmd"].as_str().unwrap_or("");
    let service = req["service"].as_str().unwrap_or("sing-box");

    match cmd {
        "ping" => cmd_ping(),
        "status" => cmd_status(service),
        "start" => {
            let config_path = req["config_path"].as_str().unwrap_or("");
            cmd_start(service, config_path)
        }
        "stop" => cmd_stop(service),
        "restart" => {
            cmd_stop(service);
            thread::sleep(Duration::from_millis(500));
            let config_path = req["config_path"].as_str().unwrap_or("");
            cmd_start(service, config_path)
        }
        "install" => {
            let exe = req["exe_path"].as_str().unwrap_or("");
            let singbox = req["singbox_path"].as_str().unwrap_or("");
            let config = req["config_path"].as_str().unwrap_or("");
            let workdir = req["working_dir"].as_str().unwrap_or("");
            let errlog = req["error_log"].as_str().unwrap_or("");
            cmd_install(service, exe, singbox, config, workdir, errlog)
        }
        "uninstall" => cmd_uninstall(service),
        "clear_proxy" => cmd_clear_proxy(),
        _ => serde_json::json!({"ok": false, "error": format!("未知指令: {}", cmd)}),
    }
}

// ── 主循环 ───────────────────────────────────────────────────────────────────

fn main() {
    start_network_watcher();

    let _ = fs::remove_file(SOCKET_PATH);

    let listener = match UnixListener::bind(SOCKET_PATH) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[helper] 绑定 socket 失败: {}", e);
            std::process::exit(1);
        }
    };

    let _ = Command::new("chmod").args(["666", SOCKET_PATH]).output();

    eprintln!("[helper] 启动，监听 {}", SOCKET_PATH);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    let stream_clone = match stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let reader = BufReader::new(stream_clone);
                    let mut writer = stream;
                    for line in reader.lines() {
                        match line {
                            Ok(line) if !line.trim().is_empty() => {
                                let resp = match serde_json::from_str::<serde_json::Value>(&line) {
                                    Ok(req) => dispatch(&req),
                                    Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
                                };
                                let mut resp_str = resp.to_string();
                                resp_str.push('\n');
                                let _ = writer.write_all(resp_str.as_bytes());
                            }
                            _ => break,
                        }
                    }
                });
            }
            Err(e) => eprintln!("[helper] 连接错误: {}", e),
        }
    }
}
