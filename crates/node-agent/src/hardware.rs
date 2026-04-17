use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub gpu_name: String,
    pub vram_mb: u64,
    pub cpu_count: usize,
    pub total_ram_mb: u64,
}

pub fn collect() -> HardwareInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_count = sys.cpus().len();
    let total_ram_mb = sys.total_memory() / 1024 / 1024;

    // Try to detect GPU via sysinfo or env override
    // Real GPU detection requires platform-specific libs (nvml, metal)
    // For MVP, check common env vars then fall back to a placeholder
    let (gpu_name, vram_mb) = detect_gpu();

    HardwareInfo {
        gpu_name,
        vram_mb,
        cpu_count,
        total_ram_mb,
    }
}

fn detect_gpu() -> (String, u64) {
    // Honour explicit env overrides (useful for nodes without nvml access)
    if let (Ok(name), Ok(vram)) = (
        std::env::var("GPU_NAME"),
        std::env::var("GPU_VRAM_MB"),
    ) {
        if let Ok(vram_mb) = vram.parse::<u64>() {
            return (name, vram_mb);
        }
    }

    // Attempt to read from /proc/driver/nvidia/gpus on Linux
    #[cfg(target_os = "linux")]
    if let Some(info) = probe_nvidia_proc() {
        return info;
    }

    // Apple Silicon — report unified memory as VRAM
    #[cfg(target_os = "macos")]
    {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_all();
        let total_mb = sys.total_memory() / 1024 / 1024;
        // Assume half of unified memory is available for GPU
        return ("Apple Silicon (unified)".to_string(), total_mb / 2);
    }

    #[allow(unreachable_code)]
    ("Unknown GPU".to_string(), 0)
}

#[cfg(target_os = "linux")]
fn probe_nvidia_proc() -> Option<(String, u64)> {
    // /proc/driver/nvidia/gpus/<uuid>/information contains "Model: ..." and "Video Memory: N MiB"
    let dir = std::fs::read_dir("/proc/driver/nvidia/gpus").ok()?;
    for entry in dir.flatten() {
        let info_path = entry.path().join("information");
        let content = std::fs::read_to_string(info_path).ok()?;
        let mut gpu_name = None;
        let mut vram_mb = None;
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("Model:") {
                gpu_name = Some(rest.trim().to_string());
            }
            if let Some(rest) = line.strip_prefix("Video Memory:") {
                // "N MiB"
                if let Some(n) = rest.trim().split_whitespace().next() {
                    vram_mb = n.parse::<u64>().ok();
                }
            }
        }
        if let (Some(name), Some(vram)) = (gpu_name, vram_mb) {
            return Some((name, vram));
        }
    }
    None
}
