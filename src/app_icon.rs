//! The approved D101 light icon is embedded in both the executable and the UI.
use std::{fs, io, path::Path, sync::OnceLock};
use windows_reactor::*;

static WINDOW_ICON: OnceLock<String> = OnceLock::new();
const ICO: &[u8] = include_bytes!("../assets/app-icon.ico");

/// AppWindow.SetIcon requires a real ICO path. Keep a copy in the app's own
/// data directory so a portable executable never depends on its working dir.
pub fn prepare(directory: &Path) -> io::Result<()> {
    let assets = std::path::absolute(directory)?.join("assets");
    fs::create_dir_all(&assets)?;
    let path = assets.join("d101-app-light.ico");
    if fs::read(&path).ok().as_deref() != Some(ICO) {
        fs::write(&path, ICO)?;
    }
    let _ = WINDOW_ICON.set(path.to_string_lossy().into_owned());
    Ok(())
}

pub fn window_path() -> Option<&'static str> {
    WINDOW_ICON.get().map(String::as_str)
}

pub fn view(size: f64) -> Image {
    Image::new()
        .source_data(EncodedImage::from_static(include_bytes!(
            "../assets/app-icon.png"
        )))
        .width(size)
        .height(size)
        .stretch(Stretch::Uniform)
        .vertical_alignment(VerticalAlignment::Center)
}
