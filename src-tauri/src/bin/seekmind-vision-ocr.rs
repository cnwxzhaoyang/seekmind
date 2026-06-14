/*
 * @author MorningSun
 * @CreatedDate 2026/06/06
 * @Description SeekMind bundled OCR helper entry point for supported desktop platforms.
 */

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = seekmind_lib::run_vision_ocr_helper(&args) {
        eprintln!("[SeekMind][VisionOCR] helper failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("[SeekMind][VisionOCR] helper is only available on macOS and Windows");
    std::process::exit(1);
}
