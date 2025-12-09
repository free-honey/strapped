use anyhow::{
    Context,
    Result,
    ensure,
};
use clap::{
    Parser,
    Subcommand,
};
use std::{
    path::Path,
    process::Command,
};

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "Strapped helper tasks (build Sway, regen ABI, clippy, integration tests)",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build Sway contracts (release) to refresh binaries/ABIs
    BuildSway,
    /// Rebuild Sway contracts and recompile generated_abi without a full cargo clean
    Abi {
        /// Skip rebuilding the Sway contracts first
        #[arg(long)]
        skip_sway: bool,
    },
    /// Run clippy for the entire workspace with warnings-as-errors
    Clippy,
    /// Run integration tests (optionally skipping rebuild steps)
    Test {
        /// Skip rebuilding Sway contracts
        #[arg(long)]
        skip_sway: bool,
        /// Skip recompiling generated_abi (useful if already up-to-date)
        #[arg(long)]
        skip_abi: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = repo_root();

    match cli.command {
        Commands::BuildSway => build_sway(&root)?,
        Commands::Abi { skip_sway } => {
            if !skip_sway {
                build_sway(&root)?;
            }
            build_generated_abi(&root)?;
        }
        Commands::Clippy => run_clippy(&root)?,
        Commands::Test {
            skip_sway,
            skip_abi,
        } => {
            if !skip_sway {
                build_sway(&root)?;
            }
            if !skip_abi {
                build_generated_abi(&root)?;
            }
            run_integration_tests(&root)?;
        }
    }

    Ok(())
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has no parent directory")
        .to_path_buf()
}

fn build_sway(root: &Path) -> Result<()> {
    let projects = [
        "sway-projects/strapped",
        "sway-projects/pseudo-vrf-contract",
        "sway-projects/fake-vrf-contract",
    ];

    for project in projects {
        let path = root.join(project);
        ensure!(path.exists(), "missing Sway project at {}", path.display());
        let mut cmd = Command::new("forc");
        cmd.arg("build").arg("--release").current_dir(&path);
        run_command(cmd, &format!("forc build ({})", project))?;
    }

    Ok(())
}

fn build_generated_abi(root: &Path) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("check")
        .arg("-p")
        .arg("generated_abi")
        .arg("--quiet")
        .current_dir(root);
    run_command(cmd, "cargo check -p generated_abi")?;
    Ok(())
}

fn run_clippy(root: &Path) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("clippy")
        .arg("--workspace")
        .arg("--all-targets")
        .arg("--all-features")
        .arg("--")
        .arg("-D")
        .arg("warnings")
        .current_dir(root);
    run_command(cmd, "cargo clippy")?;
    Ok(())
}

fn run_integration_tests(root: &Path) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("test")
        .arg("-p")
        .arg("integration-tests")
        .current_dir(root);
    run_command(cmd, "cargo test -p integration-tests")?;
    Ok(())
}

fn run_command(mut cmd: Command, label: &str) -> Result<()> {
    println!("Running: {}", label);
    let status = cmd
        .status()
        .with_context(|| format!("failed to run {label}"))?;
    ensure!(status.success(), "{label} failed with status {status}");
    Ok(())
}
