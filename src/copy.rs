use std::fs;
use std::path::Path;

use crate::output::*;

fn destination_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn warn_replacing_destination(op: &str, rel: &str, dst: &Path) {
    eprintln!(
        "{} destination exists; removing before {}: {} ({})",
        YELLOW.paint("⚠"),
        op,
        rel,
        dst.display(),
    );
}

/// Remove a file, symlink, or directory at `path` so a copy or symlink can replace it.
fn remove_existing_destination(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
        Ok(md) => {
            let ft = md.file_type();
            if ft.is_dir() && !ft.is_symlink() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            }
        }
    }
}

fn create_symlink(target: &Path, link: &Path, src_is_dir: bool) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let _ = src_is_dir;
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        if src_is_dir {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            std::os::windows::fs::symlink_file(target, link)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symlinks are only supported on Unix and Windows",
        ))
    }
}

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
/// Missing source entries are silently skipped. If the destination path already exists, a warning
/// is printed to stderr, then it is removed (file, symlink, or directory tree) so the copy replaces
/// it—same as [`link_paths`]. Copy failures print a warning but do not abort the operation.
/// Returns the list of paths that were successfully copied.
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

        if destination_exists(&dst) {
            warn_replacing_destination("copy", rel, &dst);
        }

        verbose!("copy_paths: copying '{}' ({} -> {})", rel, src.display(), dst.display());
        println!("  {} {}", DIM.paint("copying"), rel);

        if let Err(e) = remove_existing_destination(&dst) {
            eprintln!(
                "{} could not remove existing destination for copy {} ({}): {}",
                YELLOW.paint("⚠"),
                rel,
                dst.display(),
                e,
            );
            continue;
        }

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
                println!("  {} {}", DIM.paint("copied"), rel);
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

/// Create a symlink at `dest`/`<rel>` pointing to the canonical path of `source`/`<rel>`.
///
/// Missing source entries are silently skipped. If the destination path already exists, a warning
/// is printed to stderr, then it is removed (file, symlink, or directory tree) so the symlink can
/// replace it. Failures print a warning. Returns the list of paths that were successfully linked.
pub fn link_paths(source: &Path, dest: &Path, paths: &[String]) -> Vec<String> {
    verbose!("link_paths: source={}, dest={}", source.display(), dest.display());
    verbose!("link_paths: paths={:?}", paths);

    let mut linked = Vec::new();

    for rel in paths {
        let src = source.join(rel);
        let dst = dest.join(rel);

        if !src.exists() {
            verbose!(
                "link_paths: skipping '{}' — source does not exist ({})",
                rel,
                src.display()
            );
            continue;
        }

        if destination_exists(&dst) {
            warn_replacing_destination("link", rel, &dst);
        }

        println!("  {} {}", DIM.paint("linking"), rel);

        if let Err(e) = remove_existing_destination(&dst) {
            eprintln!(
                "{} could not remove existing destination for symlink {} ({}): {}",
                YELLOW.paint("⚠"),
                rel,
                dst.display(),
                e,
            );
            continue;
        }

        let target = match src.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "{} failed to resolve source for symlink {}: {}",
                    YELLOW.paint("⚠"),
                    rel,
                    e,
                );
                continue;
            }
        };

        verbose!(
            "link_paths: linking '{}' ({} -> {})",
            rel,
            dst.display(),
            target.display()
        );

        let src_is_dir = src.is_dir();
        let result = if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).and_then(|_| create_symlink(&target, &dst, src_is_dir))
        } else {
            create_symlink(&target, &dst, src_is_dir)
        };

        match result {
            Ok(()) => {
                verbose!("link_paths: '{}' linked successfully", rel);
                println!("  {} {}", DIM.paint("linked"), rel);
                linked.push(rel.clone());
            }
            Err(e) => {
                eprintln!(
                    "{} failed to symlink {}: {}",
                    YELLOW.paint("⚠"),
                    rel,
                    e,
                );
            }
        }
    }

    linked
}
