//! Lists the sprite files under assets/sprites at compile time, so the web
//! build (which can't look at the disk) only requests sprites that exist
//! (`game::assets::sprite_exists`).

use std::path::Path;

fn main() {
    let sprites = Path::new("assets/sprites");
    println!("cargo:rerun-if-changed={}", sprites.display());
    let mut files = Vec::new();
    collect(sprites, &mut files);
    files.sort();
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("sprite_files.txt");
    std::fs::write(out, files.join("\n")).unwrap();
}

/// Every PNG below `dir`, as an asset path ("sprites/units/western/scout.png")
fn collect(dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "png") {
            let asset = path.strip_prefix("assets").unwrap();
            files.push(asset.to_string_lossy().replace('\\', "/"));
        }
    }
}
