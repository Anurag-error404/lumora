//! `lumora-vault` — open a LUMORA locked vault folder without the app.
//!
//! Copy this binary next to the vault folder (e.g. on an external drive) and
//! restore its contents on any machine:
//!
//! ```text
//! lumora-vault unlock --vault /Volumes/Drive/Locked --out ~/Desktop/restored
//! lumora-vault unlock --vault … --out … --password 'hunter2'
//! lumora-vault unlock --vault … --out … --recovery '4T9K-…'
//! lumora-vault list   --vault /Volumes/Drive/Locked
//! ```
//!
//! With no `--password` / `--recovery`, the password is read from the
//! `LUMORA_VAULT_PASSWORD` environment variable or prompted for on the
//! terminal. The vault is never modified: files are decrypted into `--out`
//! and the encrypted originals stay in place.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use photovault_ai_lib::error::{AppError, AppResult};
use photovault_ai_lib::portable::{self, Secret};

#[derive(Parser)]
#[command(
    name = "lumora-vault",
    about = "Unlock a LUMORA vault folder without the app",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Decrypt everything in a vault folder into an output directory.
    Unlock {
        #[command(flatten)]
        vault: VaultArgs,
        /// Directory to write the restored files into (created if missing).
        #[arg(short, long)]
        out: PathBuf,
    },
    /// List what a vault contains, without writing anything.
    List {
        #[command(flatten)]
        vault: VaultArgs,
    },
}

#[derive(Args)]
struct VaultArgs {
    /// Path to the vault folder (the one holding vault.json and blobs/).
    #[arg(short, long)]
    vault: PathBuf,
    /// Vault password. Prompted for if omitted.
    #[arg(short, long, conflicts_with = "recovery")]
    password: Option<String>,
    /// One-time recovery code, used instead of the password.
    #[arg(short, long)]
    recovery: Option<String>,
}

const PASSWORD_ENV: &str = "LUMORA_VAULT_PASSWORD";

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Unlock { vault, out } => unlock(vault, out),
        Command::List { vault } => list(vault),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Resolve the secret once so the (possibly prompted) password outlives the
/// borrow taken by [`Secret`].
fn resolve_secret(args: &VaultArgs) -> AppResult<(String, bool)> {
    if let Some(code) = &args.recovery {
        return Ok((code.clone(), true));
    }
    if let Some(password) = &args.password {
        return Ok((password.clone(), false));
    }
    if let Ok(password) = std::env::var(PASSWORD_ENV) {
        if !password.is_empty() {
            return Ok((password, false));
        }
    }
    let password = rpassword::prompt_password("Vault password: ")
        .map_err(|e| AppError::msg(format!("could not read password: {e}")))?;
    if password.is_empty() {
        return Err(AppError::msg("no password entered"));
    }
    Ok((password, false))
}

fn vault_path(path: &Path) -> AppResult<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::msg("vault path is not valid UTF-8"))
}

fn unlock(args: VaultArgs, out: PathBuf) -> AppResult<()> {
    let vault = vault_path(&args.vault)?;
    let (secret, is_recovery) = resolve_secret(&args)?;
    let secret = if is_recovery {
        Secret::RecoveryCode(&secret)
    } else {
        Secret::Password(&secret)
    };

    let summary = portable::unlock_to_dir(&vault, &secret, &out)?;
    println!("restored {} file(s) to {}", summary.restored, out.display());
    for error in &summary.errors {
        eprintln!("  skipped: {error}");
    }
    if summary.restored == 0 && !summary.errors.is_empty() {
        return Err(AppError::msg("nothing could be restored"));
    }
    Ok(())
}

fn list(args: VaultArgs) -> AppResult<()> {
    let vault = vault_path(&args.vault)?;
    let (secret, is_recovery) = resolve_secret(&args)?;
    let secret = if is_recovery {
        Secret::RecoveryCode(&secret)
    } else {
        Secret::Password(&secret)
    };

    let manifest = portable::read_manifest(&vault)?;
    let key = portable::unwrap_key(&manifest, &secret)?;
    let catalog = portable::read_catalog(&vault, &key)?;

    println!("{} item(s) in {vault}", catalog.items.len());
    for item in &catalog.items {
        let group = item
            .album_id
            .as_ref()
            .and_then(|id| catalog.albums.iter().find(|a| &a.id == id))
            .map(|a| format!("{}/", a.name))
            .unwrap_or_default();
        println!("  {group}{}", item.rel_path);
    }
    Ok(())
}
