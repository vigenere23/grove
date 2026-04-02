use std::fs;
use std::path::Path;

use crate::output::*;

fn copy_dir_recursive_fallback(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive_fallback(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

fn try_os_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        let output = std::process::Command::new("cp")
            .arg("-R")
            .arg(src)
            .arg(dst)
            .output()?;

        return if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("cp -R failed: {}", stderr.trim()),
            ))
        } else {
            Ok(())
        };
    }

    #[cfg(windows)]
    {
        let output = std::process::Command::new("robocopy")
            .arg(src)
            .arg(dst)
            .args(["/E", "/NFL", "/NDL", "/NJH", "/NJS", "/NP"])
            .output()?;

        let code = output.status.code().unwrap_or(8);
        return if code >= 8 {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("robocopy failed (exit {}): {}", code, stdout.trim()),
            ))
        } else {
            Ok(())
        };
    }

    #[allow(unreachable_code)]
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no native copy command available on this platform",
    ))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    match try_os_copy(src, dst) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!(
                "{} fast copy failed ({}), falling back to manual copy",
                YELLOW.paint("⚠"),
                e,
            );
            copy_dir_recursive_fallback(src, dst)
        }
    }
}

/// Copy each path in `paths` from `source` into `dest`.
///
/// Missing source entries are silently skipped. Copy failures print a warning
/// but do not abort the operation. Returns the list of paths that were
/// successfully copied.
pub fn copy_paths(source: &Path, dest: &Path, paths: &[String]) -> Vec<String> {
    verbose!("copy_paths: source={}, dest={}", source.display(), dest.display());
    verbose!("copy_paths: paths={:?}", paths);

    let mut copied = Vec::new();

    for rel in paths {
        let src = source.join(rel);
        let dst = dest.join(rel);

        if !src.exists() {
            verbose!("copy_paths: skipping '{}' — source does not exist ({})", rel, src.display());
            continue;
        }

        verbose!("copy_paths: copying '{}' ({} -> {})", rel, src.display(), dst.display());

        let result = if src.is_dir() {
            copy_dir_recursive(&src, &dst)
        } else {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).and_then(|_| {
                    fs::copy(&src, &dst)?;
                    Ok(())
                })
            } else {
                fs::copy(&src, &dst).map(|_| ())
            }
        };

        match result {
            Ok(()) => {
                verbose!("copy_paths: '{}' copied successfully", rel);
                copied.push(rel.clone());
            }
            Err(e) => {
                eprintln!(
                    "{} failed to copy {}: {}",
                    YELLOW.paint("⚠"),
                    rel,
                    e,
                );
            }
        }
    }

    copied
}
