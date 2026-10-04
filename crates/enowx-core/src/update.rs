//! Finding a newer enx release and installing it in place of the running
//! binary: `enx update`, `/update`, and the check the interface makes when
//! it starts.
//!
//! A release is the archive for this platform from the GitHub release,
//! checked against the `.sha256` published beside it before anything is
//! written. The new binary replaces the old one by a rename in the same
//! folder, so a failure at any point leaves the old one working.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context as _, Result};

/// The repository enx is released from.
pub const REPO: &str = "enowdev/enowxcli";

/// The version running now.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The release target this binary was built for, as the release archives
/// are named; None on a platform no release is built for.
pub fn target() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        _ => return None,
    })
}

/// A version's numbers: `v0.2.1` and `0.2.1` alike; a pre-release suffix
/// (`-rc.1`) is left out of the comparison.
fn numbers(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Whether `latest` is newer than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let (mut a, mut b) = (numbers(latest), numbers(current));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("enx/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .build()?)
}

/// The latest release's tag (`v0.2.1`). Asks enowx.ai first, which caches
/// GitHub's answer, then GitHub itself.
pub async fn latest() -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Site {
        version: String,
    }
    #[derive(serde::Deserialize)]
    struct GitHub {
        tag_name: String,
    }
    let http = client()?;
    let quick = async {
        http.get("https://enowx.ai/api/latest-release")
            .timeout(Duration::from_secs(8))
            .send()
            .await?
            .error_for_status()?
            .json::<Site>()
            .await
    };
    if let Ok(site) = quick.await {
        if !site.version.is_empty() {
            return Ok(site.version);
        }
    }
    let release: GitHub = http
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .context("asking GitHub for the latest release")?
        .error_for_status()
        .context("asking GitHub for the latest release")?
        .json()
        .await
        .context("reading GitHub's answer")?;
    Ok(release.tag_name)
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    UpToDate,
    Available(String),
}

/// Compare the latest release with this one.
pub async fn check() -> Result<Check> {
    let latest = latest().await?;
    Ok(if is_newer(&latest, current()) {
        Check::Available(latest)
    } else {
        Check::UpToDate
    })
}

/// Whether the check at start is turned off by the environment.
pub fn check_disabled_by_env() -> bool {
    std::env::var_os("ENX_NO_UPDATE_CHECK").is_some_and(|v| !v.is_empty())
}

/// The SHA-256 of `bytes`, in lowercase hex.
fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Download release `tag` for this platform, verify it, and put its binary
/// in place of the running one. The path written; the new version runs
/// from the next start.
pub async fn install(tag: &str) -> Result<PathBuf> {
    let target = target().context("no release is built for this platform; build from source")?;
    let archive = if cfg!(windows) {
        format!("enowx-{target}.zip")
    } else {
        format!("enowx-{target}.tar.gz")
    };
    let base = format!("https://github.com/{REPO}/releases/download/{tag}");
    let http = client()?;
    let bytes = http
        .get(format!("{base}/{archive}"))
        .send()
        .await
        .with_context(|| format!("downloading {archive}"))?
        .error_for_status()
        .with_context(|| format!("downloading {archive}"))?
        .bytes()
        .await?;
    let expected = http
        .get(format!("{base}/{archive}.sha256"))
        .send()
        .await
        .context("downloading the checksum")?
        .error_for_status()
        .context("downloading the checksum")?
        .text()
        .await?;
    let expected = expected
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let actual = sha256_hex(&bytes);
    if expected.len() != 64 || actual != expected {
        bail!("{archive} does not match its published checksum; nothing was installed");
    }

    let exe = std::env::current_exe().context("finding the running enowx")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let folder = exe
        .parent()
        .context("the running enowx has no folder")?
        .to_path_buf();
    let work = folder.join(format!(".enx-update-{}", std::process::id()));
    std::fs::create_dir_all(&work)
        .with_context(|| format!("writing in {} (is it yours?)", folder.display()))?;
    let outcome = unpack_and_replace(&bytes, &archive, target, &work, &exe);
    let _ = std::fs::remove_dir_all(&work);
    outcome?;
    link_names(&exe)?;
    Ok(exe)
}

/// The two names the command answers to. Both run the same binary: `enowx`,
/// and `enx`, which it was called before.
const NAMES: [&str; 2] = ["enowx", "enx"];

/// Make the other name beside `exe` run what `exe` now holds, whichever
/// name was updated: a symlink on macOS and Linux, a copy on Windows (which
/// cannot rename over a running .exe, but can move it aside). An older binary
/// under the other name is replaced, so no old version is left to run.
pub fn link_names(exe: &Path) -> Result<()> {
    let folder = exe.parent().context("the binary has no folder")?;
    let own = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_owned();
    for name in NAMES.iter().filter(|n| **n != own) {
        #[cfg(unix)]
        {
            let link = folder.join(name);
            let target = exe.file_name().context("the binary has no name")?;
            let already = std::fs::read_link(&link).is_ok_and(|t| t == Path::new(target));
            if already {
                continue;
            }
            let staged = folder.join(format!(".{name}-link-{}", std::process::id()));
            let _ = std::fs::remove_file(&staged);
            std::os::unix::fs::symlink(target, &staged)
                .with_context(|| format!("linking {name} in {}", folder.display()))?;
            std::fs::rename(&staged, &link).with_context(|| format!("putting {name} in place"))?;
        }
        #[cfg(windows)]
        {
            let other = folder.join(format!("{name}.exe"));
            let staged = folder.join(format!("{name}.new.exe"));
            std::fs::copy(exe, &staged).with_context(|| format!("copying to {name}.exe"))?;
            if other.exists() {
                let old = folder.join(format!("{name}.old.exe"));
                let _ = std::fs::remove_file(&old);
                std::fs::rename(&other, &old)
                    .with_context(|| format!("moving the old {name}.exe aside"))?;
            }
            std::fs::rename(&staged, &other)
                .with_context(|| format!("putting {name}.exe in place"))?;
        }
    }
    Ok(())
}

fn unpack_and_replace(
    bytes: &[u8],
    archive: &str,
    target: &str,
    work: &Path,
    exe: &Path,
) -> Result<()> {
    let archive_path = work.join(archive);
    std::fs::write(&archive_path, bytes)?;
    // `tar` reads both: the tar.gz on macOS and Linux, the zip on Windows
    // (bsdtar ships with Windows 10 and later).
    let status = std::process::Command::new("tar")
        .arg("-xf")
        .arg(&archive_path)
        .arg("-C")
        .arg(work)
        .status()
        .context("running tar to unpack the release")?;
    if !status.success() {
        bail!("tar could not unpack {archive}");
    }
    let name = if cfg!(windows) { "enowx.exe" } else { "enowx" };
    let fresh = work.join(format!("enowx-{target}")).join(name);
    if !fresh.is_file() {
        bail!("{archive} has no {name}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755))?;
    }
    // Staged beside the running binary, then renamed over it: a rename in
    // one folder is atomic, and the running process keeps the old file.
    let staged = exe.with_extension("new");
    std::fs::copy(&fresh, &staged)?;
    #[cfg(windows)]
    {
        // Windows will not replace a running .exe, but will rename it.
        let old = exe.with_extension("old.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old).context("moving the running enowx aside")?;
        if let Err(error) = std::fs::rename(&staged, exe) {
            let _ = std::fs::rename(&old, exe);
            return Err(error).context("putting the new enowx in place");
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(&staged, exe).context("putting the new enowx in place")?;
    Ok(())
}

/// Remove what a Windows update left beside the binary (the old .exe, which
/// could not be deleted while it ran). Harmless elsewhere.
pub fn clean_up() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old.exe"));
        if let Some(folder) = exe.parent() {
            for name in NAMES {
                let _ = std::fs::remove_file(folder.join(format!("{name}.old.exe")));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(is_newer("v0.2.1", "0.2.0"));
        assert!(is_newer("v0.10.0", "0.9.9"), "numbers, not text");
        assert!(is_newer("1.0", "0.99.0"));
        assert!(!is_newer("v0.2.0", "0.2.0"));
        assert!(!is_newer("v0.1.9", "0.2.0"));
        assert!(
            !is_newer("v0.2.0-rc.1", "0.2.0"),
            "a pre-release is not newer"
        );
    }

    /// Updating either name leaves the other running the same binary, and
    /// an old binary under the other name is replaced.
    #[cfg(unix)]
    #[test]
    fn both_names_run_the_updated_binary() {
        let dir = std::env::temp_dir().join(format!("enowx-names-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let enowx = dir.join("enowx");
        std::fs::write(&enowx, "new").unwrap();
        std::fs::write(dir.join("enx"), "old binary").unwrap();
        link_names(&enowx).unwrap();
        assert_eq!(
            std::fs::read_link(dir.join("enx")).unwrap(),
            Path::new("enowx")
        );
        assert_eq!(std::fs::read_to_string(dir.join("enx")).unwrap(), "new");
        link_names(&enowx).unwrap();
        assert_eq!(
            std::fs::read_link(dir.join("enx")).unwrap(),
            Path::new("enowx")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn this_platform_has_a_release_target() {
        let target = target().expect("a release target");
        assert!(target.contains(std::env::consts::ARCH));
    }

    #[test]
    fn the_checksum_is_sha256_in_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
