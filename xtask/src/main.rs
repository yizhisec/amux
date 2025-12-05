use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build tasks for ccm project")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install ccm and ccm-daemon to a bin directory
    Install {
        /// Custom installation path (default: ~/.cargo/bin or ~/.local/bin)
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { path } => install(path),
    }
}

fn install(custom_path: Option<PathBuf>) -> Result<()> {
    // Build release binaries
    println!("Building release binaries...");
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "ccm-cli", "-p", "ccm-daemon"])
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        bail!("Build failed");
    }

    // Determine install directory
    let install_dir = determine_install_dir(custom_path)?;
    println!("Installing to: {}", install_dir.display());

    // Create install directory if needed
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("Failed to create directory: {}", install_dir.display()))?;

    // Get project root (where Cargo.toml is)
    let project_root = project_root()?;
    let target_dir = project_root.join("target/release");

    // Copy binaries
    let binaries = [("ccm", "ccm"), ("ccm-daemon", "ccm-daemon")];

    for (src_name, dst_name) in binaries {
        let src = target_dir.join(src_name);
        let dst = install_dir.join(dst_name);

        if !src.exists() {
            bail!("Binary not found: {}", src.display());
        }

        fs::copy(&src, &dst)
            .with_context(|| format!("Failed to copy {} to {}", src.display(), dst.display()))?;

        // Set executable permissions (0o755)
        let mut perms = fs::metadata(&dst)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dst, perms)?;

        println!("  Installed: {}", dst.display());
    }

    // Check if install_dir is in PATH
    if let Ok(path_env) = std::env::var("PATH") {
        let install_dir_str = install_dir.to_string_lossy();
        if !path_env.split(':').any(|p| p == install_dir_str) {
            println!();
            println!("Note: {} is not in your PATH.", install_dir.display());
            println!("Add it to your shell config:");
            println!("  export PATH=\"{}:$PATH\"", install_dir.display());
        }
    }

    println!();
    println!("Installation complete!");

    Ok(())
}

fn determine_install_dir(custom_path: Option<PathBuf>) -> Result<PathBuf> {
    // Priority: custom path > ~/.cargo/bin > ~/.local/bin
    if let Some(path) = custom_path {
        return Ok(path);
    }

    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let home = PathBuf::from(home);

    // Check ~/.cargo/bin first
    let cargo_bin = home.join(".cargo/bin");
    if cargo_bin.exists() {
        return Ok(cargo_bin);
    }

    // Fallback to ~/.local/bin
    let local_bin = home.join(".local/bin");
    if local_bin.exists() {
        return Ok(local_bin);
    }

    // If neither exists, default to ~/.cargo/bin (will be created)
    Ok(cargo_bin)
}

fn project_root() -> Result<PathBuf> {
    // xtask binary is in target/debug or target/release
    // We need to find the workspace root
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok();

    if let Some(dir) = manifest_dir {
        // When run via `cargo run`, CARGO_MANIFEST_DIR points to xtask/
        let path = PathBuf::from(dir);
        if let Some(parent) = path.parent() {
            return Ok(parent.to_path_buf());
        }
    }

    // Fallback: use current directory and search for workspace Cargo.toml
    let mut current = std::env::current_dir()?;
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = fs::read_to_string(&cargo_toml)?;
            if content.contains("[workspace]") {
                return Ok(current);
            }
        }
        if !current.pop() {
            bail!("Could not find workspace root");
        }
    }
}
