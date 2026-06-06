use std::path::Path;

fn main() {
    tauri_build::build();

    // Copy manifest + skills to target directory for dev builds.
    // The manifest declares UTF-8 code page for ALL child processes on Windows.
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let target_root = Path::new(&out_dir)
        .parent().unwrap().parent().unwrap().parent().unwrap();

    let manifest = Path::new("minimax-code.exe.manifest");
    if manifest.exists() {
        let _ = std::fs::copy(manifest, target_root.join("minimax-code.exe.manifest"));
    }

    let skills_src = Path::new("skills");
    if skills_src.exists() {
        let skills_dst = target_root.join("skills");
        if let Err(e) = copy_dir(skills_src, &skills_dst) {
            println!("cargo:warning=Failed to copy skills: {}", e);
        }
    }
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}
