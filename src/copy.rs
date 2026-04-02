use std::fs;
use std::path::Path;

use crate::output::*;

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
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
