//! `--shot <path>`: capture the rendered frame and exit.

use std::path::Path;

use macroquad::prelude::get_screen_data;

/// Write the current back buffer to a PNG. The buffer keeps a partial alpha on
/// macOS, so alpha is forced opaque before encoding.
pub fn capture(path: &Path) -> Result<(), String> {
    let mut image = get_screen_data();
    for pixel in image.bytes.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    let text = path.to_string_lossy().to_string();
    image.export_png(&text);
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("screenshot {} was not written", path.display()))
    }
}
