use std::env;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::SystemTime;

fn main() {
    generate_icns_from_png();
    tauri_build::build();
}

fn generate_icns_from_png() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let png_path = manifest_dir.join("icons/icon.png");
    let icns_path = manifest_dir.join("icons/icon.icns");

    if !png_path.exists() {
        return;
    }

    if icns_path.exists() && is_up_to_date(&png_path, &icns_path) {
        println!("cargo:rerun-if-changed=icons/icon.png");
        return;
    }

    let png_data = match fs::read(&png_path) {
        Ok(data) => data,
        Err(error) => {
            println!("cargo:warning=failed to read icons/icon.png: {error}");
            return;
        }
    };

    let image = match icns::Image::read_png(Cursor::new(&png_data)) {
        Ok(image) => image,
        Err(error) => {
            println!("cargo:warning=failed to decode icons/icon.png: {error}");
            return;
        }
    };

    let mut family = icns::IconFamily::new();
    if let Err(error) = family.add_icon(&image) {
        println!("cargo:warning=failed to add icon to icns family: {error}");
        return;
    }

    let mut output = Vec::new();
    if let Err(error) = family.write(Cursor::new(&mut output)) {
        println!("cargo:warning=failed to encode icon.icns: {error}");
        return;
    }

    if let Err(error) = fs::write(&icns_path, output) {
        println!("cargo:warning=failed to write icons/icon.icns: {error}");
        return;
    }

    println!("cargo:rerun-if-changed=icons/icon.png");
}

fn is_up_to_date(source: &Path, destination: &Path) -> bool {
    let Ok(source_modified) = source.metadata().and_then(|meta| meta.modified()) else {
        return false;
    };
    let Ok(dest_modified) = destination.metadata().and_then(|meta| meta.modified()) else {
        return false;
    };

    dest_modified >= source_modified
        || dest_modified >= SystemTime::UNIX_EPOCH && source_modified <= dest_modified
}
