use std::path::Path;

fn main() {
    tauri_build::build();

    // Copy builtin skills to target directory so they're available next to the exe
    let skills_src = Path::new("skills");
    if skills_src.exists() {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        // OUT_DIR is like target/release/build/<crate>/out — go up to target/release
        let target_root = Path::new(&out_dir)
            .parent().unwrap()  // build/<crate>
            .parent().unwrap()  // build
            .parent().unwrap(); // release or debug
        let skills_dst = target_root.join("skills");
        if let Err(e) = copy_dir(skills_src, &skills_dst) {
            eprintln!("cargo:warning=Failed to copy skills: {}", e);
        } else {
            println!("cargo:warning=Skills copied to {}", skills_dst.display());
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
