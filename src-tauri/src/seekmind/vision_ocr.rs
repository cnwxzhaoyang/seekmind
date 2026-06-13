/*
 * @author MorningSun
 * @CreatedDate 2026/06/06
 * @Description macOS Vision OCR helper and bundled runtime probe for SeekMind.
 */

#[cfg(not(target_os = "macos"))]
use std::path::{Path, PathBuf};

const DEFAULT_VISION_OCR_LANGS: &str = "zh-Hans,en-US";

fn parse_lang_list(raw: &str) -> Vec<String> {
    raw.split(|ch: char| matches!(ch, ',' | ' ' | '\n' | '\t' | ';'))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn default_vision_ocr_languages() -> Vec<String> {
    let configured = std::env::var("SEEKMIND_VISION_OCR_LANGS")
        .ok()
        .unwrap_or_else(|| DEFAULT_VISION_OCR_LANGS.to_string());
    let languages = parse_lang_list(&configured);
    if languages.is_empty() {
        parse_lang_list(DEFAULT_VISION_OCR_LANGS)
    } else {
        languages
    }
}

pub fn has_chinese_vision_language(languages: &[String]) -> bool {
    languages.iter().any(|lang| {
        let lowered = lang.to_lowercase();
        lowered.starts_with("zh")
            || lowered.starts_with("chi_")
            || lowered == "chi_sim"
            || lowered == "chi_tra"
    })
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{default_vision_ocr_languages, parse_lang_list};
    use crate::seekmind::runtime_paths::bundled_vision_ocr_binary_path;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::OnceLock;

    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::{AnyClass, AnyObject, NSObject};
    use objc2::AnyThread;
    use objc2::{msg_send};
    use objc2_app_kit::{NSGraphicsContext, NSImage, NSImageHintKey};
    use objc2_core_graphics::CGImage;
    use objc2_foundation::{NSArray, NSDictionary, NSError, NSRect, NSString};

    #[link(name = "Vision", kind = "framework")]
    extern "C" {}

    pub fn bundled_vision_ocr_binary() -> Option<PathBuf> {
        bundled_vision_ocr_binary_path()
    }

    fn helper_candidates() -> Vec<PathBuf> {
        let mut candidates = Vec::new();

        if let Ok(value) = std::env::var("SEEKMIND_VISION_OCR_BIN") {
            let candidate = PathBuf::from(value);
            if candidate.exists() {
                candidates.push(candidate);
            }
        }

        if let Some(candidate) = bundled_vision_ocr_binary() {
            candidates.push(candidate);
        }

        candidates.push(PathBuf::from("vision-ocr"));
        candidates
    }

    fn parse_probe_languages(output: &[u8]) -> Vec<String> {
        let stdout = String::from_utf8_lossy(output);
        let languages = parse_lang_list(&stdout);
        if languages.is_empty() {
            default_vision_ocr_languages()
        } else {
            languages
        }
    }

    pub fn available_vision_ocr_languages() -> Vec<String> {
        static CACHE: OnceLock<Vec<String>> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                for candidate in helper_candidates() {
                    let output = Command::new(&candidate).arg("--probe").output();
                    let Ok(output) = output else {
                        eprintln!(
                            "[SeekMind] Vision OCR runtime probe failed for {}",
                            candidate.display()
                        );
                        continue;
                    };

                    if !output.status.success() {
                        eprintln!(
                            "[SeekMind] Vision OCR runtime probe returned non-success for {}: {}",
                            candidate.display(),
                            output.status
                        );
                        continue;
                    }

                    let languages = parse_probe_languages(&output.stdout);
                    if !languages.is_empty() {
                        eprintln!(
                            "[SeekMind] Vision OCR runtime probe succeeded with {} languages via {}",
                            languages.len(),
                            candidate.display()
                        );
                        return languages;
                    }
                }

                eprintln!("[SeekMind] Vision OCR runtime probe found no usable helper binary");
                Vec::new()
            })
            .clone()
    }

    fn load_image(path: &Path) -> Result<Retained<NSImage>, String> {
        let file_name = path.to_string_lossy();
        let ns_path = NSString::from_str(&file_name);
        let image: Option<Retained<NSImage>> =
            unsafe { msg_send![NSImage::alloc(), initWithContentsOfFile: &*ns_path] };
        let image = image.ok_or_else(|| format!("无法载入图像：{}", path.display()))?;
        Ok(image)
    }

    fn extract_lines_from_observations(observations: &NSArray<NSObject>) -> Vec<String> {
        let mut lines = Vec::new();

        for index in 0..observations.len() {
            let observation = observations.objectAtIndex(index as usize);
            let candidates: Option<Retained<NSArray<NSObject>>> =
                unsafe { msg_send![&*observation, topCandidates: 1usize] };
            let Some(candidates) = candidates else {
                continue;
            };
            if candidates.len() == 0 {
                continue;
            }

            let candidate = candidates.objectAtIndex(0);
            let confidence: f32 = unsafe { msg_send![&*candidate, confidence] };
            let text: Retained<NSString> = unsafe { msg_send![&*candidate, string] };
            let text = text.to_string().trim().to_string();
            if text.is_empty() {
                continue;
            }

            eprintln!(
                "[SeekMind][VisionOCR] observation={} confidence={:.3} text={}",
                index + 1,
                confidence,
                text.chars().take(120).collect::<String>()
            );
            lines.push(text);
        }

        lines
    }

    pub fn recognize_image_text(path: &Path, languages: &[String]) -> Result<String, String> {
        autoreleasepool(|_| {
            let image = load_image(path)?;
            let mut proposed_rect = NSRect::default();
            let cg_image: Option<Retained<CGImage>> = unsafe {
                image.CGImageForProposedRect_context_hints(
                    &mut proposed_rect,
                    None::<&NSGraphicsContext>,
                    None::<&NSDictionary<NSImageHintKey, AnyObject>>,
                )
            };
            let cg_image =
                cg_image.ok_or_else(|| format!("无法从图像生成 CGImage：{}", path.display()))?;

            let request_cls = AnyClass::get(c"VNRecognizeTextRequest")
                .ok_or_else(|| "Vision OCR 请求类不可用".to_string())?;
            let request: Retained<NSObject> = unsafe { msg_send![request_cls, new] };
            let request_languages = if languages.is_empty() {
                default_vision_ocr_languages()
            } else {
                languages.to_vec()
            };
            let language_objects: Vec<Retained<NSString>> = request_languages
                .iter()
                .map(|lang| NSString::from_str(lang))
                .collect();
            let language_array = NSArray::from_retained_slice(&language_objects);

            // 修复：Vision OCR 采用系统框架识别文字，不再依赖外部 OCR 进程；这里显式配置语言与识别级别，避免默认行为在不同系统版本下漂移。
            unsafe {
                let _: () = msg_send![&*request, setRecognitionLanguages:&*language_array];
                let _: () = msg_send![&*request, setRecognitionLevel: 0isize];
                let _: () = msg_send![&*request, setUsesLanguageCorrection: true];
            }

            let handler_cls = AnyClass::get(c"VNSequenceRequestHandler")
                .ok_or_else(|| "Vision OCR 处理器类不可用".to_string())?;
            let handler: Retained<NSObject> = unsafe { msg_send![handler_cls, new] };
            let request_array = NSArray::from_retained_slice(&[request.clone()]);
            let mut error: *mut NSError = std::ptr::null_mut();
            let success: bool = unsafe {
                // 修复：Vision 请求的 error 参数必须显式传入 NSError**，否则 objc2 宏无法正确展开。
                msg_send![&*handler, performRequests: &*request_array, onCGImage: &*cg_image, error: &mut error]
            };
            if !success {
                if error.is_null() {
                    return Err("Vision OCR 执行失败".to_string());
                }
                let retained_error = unsafe { Retained::retain(error) }
                    .ok_or_else(|| "Vision OCR 执行失败".to_string())?;
                return Err(format!(
                    "Vision OCR 执行失败：{}",
                    retained_error.localizedDescription()
                ));
            }

            let observations: Option<Retained<NSArray<NSObject>>> = unsafe { msg_send![&*request, results] };
            let Some(observations) = observations else {
                return Ok(String::new());
            };

            let lines = extract_lines_from_observations(&observations);
            Ok(lines.join("\n"))
        })
    }

    pub fn run_cli(args: &[String]) -> Result<(), String> {
        if args.iter().any(|arg| arg == "--probe") {
            let request_cls = AnyClass::get(c"VNRecognizeTextRequest")
                .ok_or_else(|| "Vision OCR 请求类不可用".to_string())?;
            let handler_cls = AnyClass::get(c"VNSequenceRequestHandler")
                .ok_or_else(|| "Vision OCR 处理器类不可用".to_string())?;
            let _ = (request_cls, handler_cls);
            let languages = default_vision_ocr_languages();
            println!("{}", languages.join(","));
            return Ok(());
        }

        let mut image_path: Option<PathBuf> = None;
        let mut languages = default_vision_ocr_languages();

        let mut index = 0usize;
        while index < args.len() {
            match args[index].as_str() {
                "--image" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--image 需要一个路径参数".to_string())?;
                    image_path = Some(PathBuf::from(value));
                    index += 2;
                }
                "--langs" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--langs 需要一个参数".to_string())?;
                    languages = parse_lang_list(value);
                    if languages.is_empty() {
                        languages = default_vision_ocr_languages();
                    }
                    index += 2;
                }
                "--help" | "-h" => {
                    println!("usage: vision-ocr --probe | --image <path> [--langs <lang1,lang2>]");
                    return Ok(());
                }
                other => {
                    return Err(format!("不支持的参数：{other}"));
                }
            }
        }

        let image_path = image_path.ok_or_else(|| "缺少 --image 参数".to_string())?;
        let text = recognize_image_text(&image_path, &languages)?;
        print!("{}", text);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub use macos::{
    available_vision_ocr_languages, bundled_vision_ocr_binary, recognize_image_text, run_cli,
};

#[cfg(not(target_os = "macos"))]
pub fn bundled_vision_ocr_binary() -> Option<PathBuf> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn available_vision_ocr_languages() -> Vec<String> {
    Vec::new()
}

#[cfg(not(target_os = "macos"))]
pub fn recognize_image_text(_path: &Path, _languages: &[String]) -> Result<String, String> {
    Err("Vision OCR 仅在 macOS 上可用".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn run_cli(_args: &[String]) -> Result<(), String> {
    Err("Vision OCR 仅在 macOS 上可用".to_string())
}
