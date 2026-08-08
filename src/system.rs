//! System information sampling on a separate thread (sysinfo + /sys, /proc).

use std::collections::VecDeque;
use std::net::{TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// The telemetry data model (Snapshot, formatting) is shared by every
// platform and lives in lib-ng-base; this file keeps the Linux
// collectors (/sys, /proc, sysinfo) that fill it.
pub use ng_base::telemetry::{ProcEntry, Snapshot, CPU_HISTORY};

fn read_sys(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn chassis_name(code: &str) -> &'static str {
    match code {
        "3" | "4" | "6" | "7" => "DESKTOP",
        "8" | "9" | "10" | "14" => "LAPTOP",
        "11" => "HANDHELD",
        "13" => "ALL IN ONE",
        "17" | "23" => "SERVER",
        "30" => "TABLET",
        "31" | "32" => "CONVERTIBLE",
        _ => "UNKNOWN",
    }
}

fn battery() -> Option<(u8, bool)> {
    for bat in ["BAT0", "BAT1", "BATT"] {
        let base = format!("/sys/class/power_supply/{bat}");
        if let Some(cap) = read_sys(&format!("{base}/capacity")) {
            let pct = cap.parse::<u8>().ok()?;
            let charging = read_sys(&format!("{base}/status"))
                .map(|s| s == "Charging" || s == "Full")
                .unwrap_or(false);
            return Some((pct, charging));
        }
    }
    None
}

fn local_ipv4() -> Option<String> {
    // UDP connect sends no packets — it only selects the interface.
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("1.1.1.1:80").ok()?;
    Some(sock.local_addr().ok()?.ip().to_string())
}

pub fn start() -> Arc<Mutex<Snapshot>> {
    let snap = Arc::new(Mutex::new(Snapshot::default()));
    let shared = snap.clone();
    let offline_mode = std::env::var("NGTERM_OFFLINE").is_ok();

    std::thread::spawn(move || {
        use sysinfo::System;
        let mut sys = System::new_all();
        let mut networks = sysinfo::Networks::new_with_refreshed_list();
        let mut components = sysinfo::Components::new_with_refreshed_list();

        // Static data
        let manufacturer = read_sys("/sys/class/dmi/id/sys_vendor").unwrap_or("UNKNOWN".into());
        let model = read_sys("/sys/class/dmi/id/product_name").unwrap_or("UNKNOWN".into());
        let chassis = read_sys("/sys/class/dmi/id/chassis_type")
            .map(|c| chassis_name(&c).to_string())
            .unwrap_or("UNKNOWN".into());
        let hostname = System::host_name().unwrap_or("localhost".into());
        let username = std::env::var("USER").unwrap_or("user".into());
        let os_name = System::name().unwrap_or("Linux".into());
        let kernel = System::kernel_version().unwrap_or_default();

        let mut cpu_hist: Vec<VecDeque<f32>> = Vec::new();
        let mut last_net: Option<(u64, u64, Instant)> = None;
        let mut last_ping = Instant::now() - Duration::from_secs(60);
        let mut ping_ms: Option<u32> = None;
        let mut online = false;
        let mut ipv4 = local_ipv4();

        loop {
            sys.refresh_cpu();
            sys.refresh_memory();
            sys.refresh_processes();
            networks.refresh();
            components.refresh();

            let cpus = sys.cpus();
            if cpu_hist.len() != cpus.len() {
                cpu_hist = vec![VecDeque::with_capacity(CPU_HISTORY); cpus.len()];
            }
            let per_core: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
            for (h, &v) in cpu_hist.iter_mut().zip(per_core.iter()) {
                if h.len() >= CPU_HISTORY {
                    h.pop_front();
                }
                h.push_back(v);
            }
            let cpu_name = cpus
                .first()
                .map(|c| c.brand().trim().to_string())
                .unwrap_or_default();

            let temp_c = (&components)
                .into_iter()
                .find(|c| {
                    let n = c.label().to_lowercase();
                    n.contains("tctl") || n.contains("package") || n.contains("cpu")
                })
                .map(|c| c.temperature());

            // Total traffic across all interfaces except lo.
            let mut total_rx = 0u64;
            let mut total_tx = 0u64;
            let mut iface = String::new();
            let mut best = 0u64;
            for (name, data) in &networks {
                if name.as_str() == "lo" {
                    continue;
                }
                total_rx += data.total_received();
                total_tx += data.total_transmitted();
                if data.total_received() > best {
                    best = data.total_received();
                    iface = name.clone();
                }
            }
            let now = Instant::now();
            let (up_rate, down_rate) = if let Some((rx0, tx0, t0)) = last_net {
                let dt = now.duration_since(t0).as_secs_f64().max(0.001);
                (
                    (total_tx.saturating_sub(tx0)) as f64 / dt,
                    (total_rx.saturating_sub(rx0)) as f64 / dt,
                )
            } else {
                (0.0, 0.0)
            };
            last_net = Some((total_rx, total_tx, now));

            // Ping every 5 s (TCP to 1.1.1.1:80) — disabled via NGTERM_OFFLINE.
            if !offline_mode && now.duration_since(last_ping) > Duration::from_secs(5) {
                last_ping = now;
                let t0 = Instant::now();
                match TcpStream::connect_timeout(
                    &"1.1.1.1:80".parse().unwrap(),
                    Duration::from_millis(1500),
                ) {
                    Ok(_) => {
                        ping_ms = Some(t0.elapsed().as_millis() as u32);
                        online = true;
                    }
                    Err(_) => {
                        ping_ms = None;
                        online = false;
                    }
                }
                ipv4 = local_ipv4();
            }

            let mut top: Vec<ProcEntry> = sys
                .processes()
                .values()
                .map(|p| ProcEntry {
                    pid: p.pid().as_u32(),
                    name: p.name().to_string(),
                    cpu: p.cpu_usage(),
                    mem_pct: if sys.total_memory() > 0 {
                        p.memory() as f32 / sys.total_memory() as f32 * 100.0
                    } else {
                        0.0
                    },
                })
                .collect();
            let proc_count = top.len();
            top.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
            top.truncate(8);

            let load = System::load_average();

            {
                let mut s = shared.lock().unwrap();
                *s = Snapshot {
                    cpu_name: cpu_name.clone(),
                    cpu_per_core: per_core,
                    cpu_hist: cpu_hist.clone(),
                    load_avg: [load.one, load.five, load.fifteen],
                    temp_c,
                    mem_total: sys.total_memory(),
                    mem_used: sys.used_memory(),
                    swap_total: sys.total_swap(),
                    swap_used: sys.used_swap(),
                    uptime: System::uptime(),
                    top,
                    proc_count,
                    net_up_rate: up_rate,
                    net_down_rate: down_rate,
                    net_total_up: total_tx,
                    net_total_down: total_rx,
                    iface: iface.clone(),
                    ipv4: ipv4.clone(),
                    ping_ms,
                    online: if offline_mode { false } else { online },
                    battery: battery(),
                    manufacturer: manufacturer.clone(),
                    model: model.clone(),
                    chassis: chassis.clone(),
                    hostname: hostname.clone(),
                    username: username.clone(),
                    os_name: os_name.clone(),
                    kernel: kernel.clone(),
                };
            }
            std::thread::sleep(Duration::from_millis(1000));
        }
    });

    snap
}
