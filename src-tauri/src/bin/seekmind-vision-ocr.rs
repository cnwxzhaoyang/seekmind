/*
 * @author MorningSun
 * @CreatedDate 2026/06/06
 * @Description SeekMind bundled macOS Vision OCR helper entry point.
 */

#[cfg(target_os = "macos")]
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(error) = seekmind_lib::run_vision_ocr_helper(&args) {
        eprintln!("[SeekMind][VisionOCR] helper failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("[SeekMind][VisionOCR] helper is only available on macOS");
    std::process::exit(1);
}
