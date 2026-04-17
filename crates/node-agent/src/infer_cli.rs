//! infer — provider CLI for infer.ram4.dev
//!
//! Usage:
//!   infer connect --endpoint http://localhost:11434 --model llama3.1:70b --provider-token <tok>
//!   infer disconnect
//!   infer status

use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::Stdio,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

// ─── CLI ────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "infer",
    version,
    about = "infer.ram4.dev provider CLI — connect your GPU to the inference network"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect this machine's GPU to the infer network
    Connect {
        /// Ollama API endpoint to expose
        #[arg(long, default_value = "http://localhost:11434")]
        endpoint: String,

        /// Model to advertise (e.g. llama3.1:70b)
        #[arg(long)]
        model: String,

        /// Provider authentication token (INFER_PROVIDER_TOKEN env var also accepted)
        #[arg(long, env = "INFER_PROVIDER_TOKEN")]
        provider_token: String,

        /// Bore tunnel server hostname
        #[arg(long, default_value = "tunnel.infer.ram4.dev")]
        tunnel_host: String,

        /// Infer coordinator API base URL
        #[arg(long, default_value = "https://api.infer.ram4.dev")]
        coordinator_url: String,
    },

    /// Disconnect this machine from the infer network
    Disconnect,

    /// Show current connection status
    Status,
}

// ─── Persistent state ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct ConnectState {
    bore_pid: u32,
    public_host: String,
    public_port: u16,
    endpoint: String,
    model: String,
    coordinator_url: String,
    provider_token: String,
    node_name: String,
}

fn state_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".infer").join("state.json")
}

fn read_state() -> Result<ConnectState> {
    let path = state_path();
    let content = fs::read_to_string(&path)
        .with_context(|| "No active connection found. Run `infer connect` first.".to_string())?;
    serde_json::from_str(&content).context("State file is corrupt")
}

fn write_state(state: &ConnectState) -> Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

fn remove_state() {
    let _ = fs::remove_file(state_path());
}

// ─── Hardware detection ─────────────────────────────────────────────────────

fn detect_vram() -> (String, u64) {
    // NVIDIA via nvidia-smi
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let line = text.trim();
            if !line.is_empty() {
                let mut parts = line.splitn(2, ',');
                if let (Some(name), Some(vram_str)) = (parts.next(), parts.next()) {
                    let vram_mb: u64 = vram_str.trim().parse().unwrap_or(0);
                    return (name.trim().to_string(), vram_mb);
                }
            }
        }
    }

    // AMD via rocm-smi
    if let Ok(out) = std::process::Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--csv"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 3 {
                    let vram_bytes: u64 = parts[2].trim().parse().unwrap_or(0);
                    if vram_bytes > 0 {
                        return ("AMD ROCm GPU".to_string(), vram_bytes / 1024 / 1024);
                    }
                }
            }
        }
    }

    // macOS Apple Silicon via sysctl
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
        {
            if out.status.success() {
                let bytes: u64 = String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse()
                    .unwrap_or(0);
                if bytes > 0 {
                    let total_mb = bytes / 1024 / 1024;
                    // Conservative estimate: half of unified memory available for GPU workloads
                    return ("Apple Silicon (unified)".to_string(), total_mb / 2);
                }
            }
        }
    }

    ("Unknown GPU".to_string(), 0)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn parse_bore_port(line: &str) -> Option<u16> {
    // bore outputs: "... listening at <host>:<port>"
    let after = line.split("listening at").nth(1)?;
    let addr = after.trim();
    addr.rsplit(':').next()?.trim().parse().ok()
}

fn is_process_alive(pid: u32) -> bool {
    // Sends signal 0 (existence check) without actually killing
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn kill_process(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-15", &pid.to_string()])
        .output();
}

async fn validate_ollama(endpoint: &str) -> Result<()> {
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Cannot reach Ollama at {url}\nIs Ollama running?"))?;
    if !resp.status().is_success() {
        bail!("Ollama returned HTTP {}", resp.status());
    }
    Ok(())
}

async fn register_node(
    coordinator_url: &str,
    provider_token: &str,
    node_name: &str,
    public_host: &str,
    public_port: u16,
    gpu_name: &str,
    vram_mb: u64,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let payload = serde_json::json!({
        "name": node_name,
        "host": public_host,
        "port": public_port,
        "agent_port": public_port,
        "gpu_name": gpu_name,
        "vram_mb": vram_mb,
    });

    let resp = client
        .post(format!("{}/v1/internal/nodes", coordinator_url))
        .bearer_auth(provider_token)
        .json(&payload)
        .send()
        .await
        .context("Failed to reach coordinator")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Registration rejected: HTTP {} — {}", status, body);
    }

    Ok(())
}

// ─── Commands ───────────────────────────────────────────────────────────────

async fn cmd_connect(
    endpoint: String,
    model: String,
    provider_token: String,
    tunnel_host: String,
    coordinator_url: String,
) -> Result<()> {
    // 1. Validate Ollama
    println!("→ Checking Ollama at {endpoint} ...");
    validate_ollama(&endpoint).await?;
    println!("  ✓ Ollama is live");

    // Parse Ollama port from endpoint URL
    let ollama_port: u16 = endpoint
        .trim_end_matches('/')
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or(11434);

    // 2. Detect GPU
    println!("→ Detecting GPU hardware ...");
    let (gpu_name, vram_mb) = detect_vram();
    println!("  ✓ {} — {} MB VRAM", gpu_name, vram_mb);

    // 3. Spawn bore tunnel
    println!("→ Opening tunnel to {tunnel_host} ...");

    // Verify bore is installed
    if std::process::Command::new("bore")
        .arg("--version")
        .output()
        .is_err()
    {
        bail!(
            "bore not found. Install it with:\n  cargo install bore-cli\n\n\
             Then retry `infer connect`."
        );
    }

    let mut bore_proc = std::process::Command::new("bore")
        .args(["local", &ollama_port.to_string(), "--to", &tunnel_host])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn bore tunnel")?;

    // Read bore stderr to find the assigned public port (30s timeout)
    let bore_stderr = bore_proc.stderr.take().expect("stderr was piped");
    let public_port = {
        let reader = BufReader::new(bore_stderr);
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut found: Option<u16> = None;
        for line in reader.lines() {
            if std::time::Instant::now() > deadline {
                break;
            }
            let line = line?;
            if line.contains("listening at") {
                if let Some(port) = parse_bore_port(&line) {
                    found = Some(port);
                    break;
                }
            }
        }
        found.context(
            "Timed out waiting for bore to open tunnel.\n\
             Check that tunnel.infer.ram4.dev is reachable on port 7835.",
        )?
    };

    let bore_pid = bore_proc.id();
    println!("  ✓ Tunnel open — public address: {tunnel_host}:{public_port}");

    // 4. Register with coordinator
    let node_name = format!(
        "provider-{}",
        std::env::var("HOSTNAME").unwrap_or_else(|_| public_port.to_string())
    );
    println!("→ Registering node '{node_name}' ...");
    register_node(
        &coordinator_url,
        &provider_token,
        &node_name,
        &tunnel_host,
        public_port,
        &gpu_name,
        vram_mb,
    )
    .await?;
    println!("  ✓ Registered with coordinator");

    // 5. Save state so `disconnect` and `status` work
    write_state(&ConnectState {
        bore_pid,
        public_host: tunnel_host.clone(),
        public_port,
        endpoint: endpoint.clone(),
        model: model.clone(),
        coordinator_url: coordinator_url.clone(),
        provider_token: provider_token.clone(),
        node_name: node_name.clone(),
    })?;

    println!();
    println!("Connected! Node '{node_name}' is live at {tunnel_host}:{public_port}");
    println!("Model: {model}  |  GPU: {gpu_name}  |  VRAM: {vram_mb} MB");
    println!("Press Ctrl-C to disconnect gracefully.");
    println!();

    // 6. Heartbeat loop — re-register every 30s to keep the node online
    let hb_coordinator = coordinator_url.clone();
    let hb_token = provider_token.clone();
    let hb_host = tunnel_host.clone();
    let hb_gpu = gpu_name.clone();
    let hb_name = node_name.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            if let Err(e) = register_node(
                &hb_coordinator,
                &hb_token,
                &hb_name,
                &hb_host,
                public_port,
                &hb_gpu,
                vram_mb,
            )
            .await
            {
                eprintln!("Heartbeat failed: {e}");
            }
        }
    });

    // 7. Wait for Ctrl-C / SIGTERM then shut down cleanly
    tokio::signal::ctrl_c().await?;
    println!("\n→ Shutting down ...");

    kill_process(bore_pid);
    remove_state();

    println!("  ✓ Tunnel closed — node will be swept offline by coordinator");
    Ok(())
}

async fn cmd_disconnect() -> Result<()> {
    let state = read_state()?;

    println!("→ Stopping bore tunnel (PID {}) ...", state.bore_pid);
    kill_process(state.bore_pid);

    // Brief pause to let bore exit cleanly
    tokio::time::sleep(Duration::from_millis(400)).await;

    remove_state();
    println!(
        "  ✓ Disconnected — node '{}' will go offline",
        state.node_name
    );
    Ok(())
}

fn cmd_status() -> Result<()> {
    match read_state() {
        Ok(state) => {
            let alive = is_process_alive(state.bore_pid);
            println!(
                "Status:      connected{}",
                if alive {
                    ""
                } else {
                    " (tunnel dead — run disconnect)"
                }
            );
            println!("Node:        {}", state.node_name);
            println!("Tunnel:      {}:{}", state.public_host, state.public_port);
            println!("Endpoint:    {}", state.endpoint);
            println!("Model:       {}", state.model);
            println!("Coordinator: {}", state.coordinator_url);
            println!("Bore PID:    {}", state.bore_pid);
        }
        Err(_) => {
            println!("Status: not connected");
        }
    }
    Ok(())
}

// ─── Entry point ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Connect {
            endpoint,
            model,
            provider_token,
            tunnel_host,
            coordinator_url,
        } => {
            cmd_connect(
                endpoint,
                model,
                provider_token,
                tunnel_host,
                coordinator_url,
            )
            .await?;
        }
        Commands::Disconnect => {
            cmd_disconnect().await?;
        }
        Commands::Status => {
            cmd_status()?;
        }
    }

    Ok(())
}
