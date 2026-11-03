//! The machine a bench runs on, as the report names it (docs/specs/bench.md
//! §4): model names and sizes only, never a name, path or serial number.

use serde::Serialize;

/// What the report says about the machine.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Machine {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub cpu: String,
    pub cores: usize,
    pub memory_gib: u64,
    /// The adapters the windowed runs drew with, as the renderer named
    /// them.
    pub gpus: Vec<String>,
    /// The renderer the app picks by default on this OS.
    pub renderer_default: String,
    pub screen: Option<Screen>,
}

/// The screen the windowed runs opened on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

impl Machine {
    /// The OS, CPU and memory, from the system.
    pub fn probe() -> Machine {
        use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        let cpu = sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| "unknown".into());
        let renderer_default = match crate::files::Renderer::Auto.backends() {
            "tiny-skia" => "tiny-skia",
            _ => "wgpu",
        };
        Machine {
            os: std::env::consts::OS.into(),
            os_version: match (
                System::long_os_version().or_else(System::os_version),
                System::kernel_version(),
            ) {
                (Some(os), Some(k)) if !os.contains(&k) => format!("{os} ({k})"),
                (Some(os), _) => os,
                (None, Some(k)) => k,
                (None, None) => "unknown".into(),
            },
            arch: std::env::consts::ARCH.into(),
            cpu,
            cores: sys
                .physical_core_count()
                .or_else(|| std::thread::available_parallelism().ok().map(|n| n.get()))
                .unwrap_or(0),
            memory_gib: (sys.total_memory() as f64 / 1_073_741_824.0).round() as u64,
            gpus: Vec::new(),
            renderer_default: renderer_default.into(),
            screen: None,
        }
    }
}

/// The process's peak resident memory in MiB, where the OS says it
/// cheaply: the peak on Linux and Windows, the current size on macOS.
pub fn peak_rss_mib() -> Option<f64> {
    if cfg!(target_os = "linux") {
        let s = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
        let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
        return Some(kb / 1024.0);
    }
    let pid = std::process::id().to_string();
    if cfg!(windows) {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {pid}).PeakWorkingSet64"),
            ])
            .output()
            .ok()?;
        let bytes: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        return Some(bytes / 1024.0 / 1024.0);
    }
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    let kb: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb / 1024.0)
}
