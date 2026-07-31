//! Read-only helpers for browsing archived git bundles.
//!
//! The archive pipeline stores each snapshot as a single-file `git bundle`.
//! These helpers shell out to the `git` CLI (present in both runtime images)
//! to turn a bundle into a browsable file tree and to extract individual
//! blob contents — the two operations behind `/archive/{id}/tree` and
//! `/archive/{id}/blob`.
//!
//! Everything is read-only: a bundle is cloned into a private temp dir which
//! is always removed afterwards, so the preserved object is never mutated.

use std::path::Path;
use std::process::Command;

/// One entry in the archived file tree.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub size: Option<i64>,
}

/// Clone a bundle into a temp dir and run `f` against the clone.
///
/// The temp dir is removed when `f` returns, even on error.
fn with_bundle_clone<T>(
    bundle_bytes: &[u8],
    f: impl FnOnce(&Path) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let tmp = tempfile::tempdir()?;
    let bundle_path = tmp.path().join("archive.bundle");
    std::fs::write(&bundle_path, bundle_bytes)?;

    let clone_dir = tmp.path().join("checkout");
    let status = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(&bundle_path)
        .arg(&clone_dir)
        .status()
        .map_err(|e| anyhow::anyhow!("git unavailable: {e}"))?;

    if !status.success() {
        anyhow::bail!("failed to open archive bundle");
    }

    f(&clone_dir)
}

/// List the full recursive file tree of a bundle at its default HEAD.
pub fn list_bundle_tree(bundle_bytes: &[u8]) -> anyhow::Result<Vec<TreeEntry>> {
    with_bundle_clone(bundle_bytes, |clone_dir| {
        let out = Command::new("git")
            .args(["ls-tree", "-r", "-l", "HEAD"])
            .current_dir(clone_dir)
            .output()?;

        if !out.status.success() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            // Format: <mode> SP <type> SP <object> TAB <size> TAB <path>
            let mut parts = line.splitn(4, ' ');
            let _mode = parts.next();
            let Some(kind) = parts.next() else { continue };
            let _object = parts.next();
            let rest = parts.next().unwrap_or("");
            let Some(tab) = rest.find('\t') else { continue };
            let size_str = &rest[..tab];
            let path = rest[tab + 1..].trim_end().to_string();
            if path.is_empty() {
                continue;
            }
            entries.push(TreeEntry {
                path,
                kind: match kind {
                    "blob" => "blob".to_string(),
                    "tree" => "tree".to_string(),
                    _ => "other".to_string(),
                },
                size: size_str.trim().parse::<i64>().ok().filter(|s| *s >= 0),
            });
        }

        Ok(entries)
    })
}

/// Read the bytes of a single file at `path` from a bundle's default HEAD.
///
/// Returns `Ok(None)` when the path does not exist in the snapshot.
pub fn read_blob_from_bundle(bundle_bytes: &[u8], path: &str) -> anyhow::Result<Option<Vec<u8>>> {
    // Defensive: reject traversal / absolute paths before they reach git.
    if path.starts_with('/') || path.split('/').any(|seg| seg == ".." || seg == ".") {
        anyhow::bail!("invalid path: {path}");
    }

    with_bundle_clone(bundle_bytes, |clone_dir: &Path| {
        let out = Command::new("git")
            .args(["show", &format!("HEAD:{path}")])
            .current_dir(clone_dir)
            .output()?;

        if !out.status.success() {
            return Ok(None);
        }
        Ok(Some(out.stdout))
    })
}

/// Whether the `git` binary is available in this runtime.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
