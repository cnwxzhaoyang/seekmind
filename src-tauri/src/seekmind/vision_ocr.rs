/*
 * @author MorningSun
 * @CreatedDate 2026/06/06
 * @Description Cross-platform OCR helper and runtime probe for SeekMind.
 */

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
    use objc2::msg_send;
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
        let image = image.ok_or_else(|| format!("failed to load image: {}", path.display()))?;
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
                cg_image.ok_or_else(|| format!("failed to create CGImage for {}", path.display()))?;

            let request_cls = AnyClass::get(c"VNRecognizeTextRequest")
                .ok_or_else(|| "Vision OCR request class unavailable".to_string())?;
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

            // Fix: pin OCR languages and correction behavior explicitly so results stay stable across macOS versions.
            unsafe {
                let _: () = msg_send![&*request, setRecognitionLanguages:&*language_array];
                let _: () = msg_send![&*request, setRecognitionLevel: 0isize];
                let _: () = msg_send![&*request, setUsesLanguageCorrection: true];
            }

            let handler_cls = AnyClass::get(c"VNSequenceRequestHandler")
                .ok_or_else(|| "Vision OCR handler class unavailable".to_string())?;
            let handler: Retained<NSObject> = unsafe { msg_send![handler_cls, new] };
            let request_array = NSArray::from_retained_slice(&[request.clone()]);
            let mut error: *mut NSError = std::ptr::null_mut();
            let success: bool = unsafe {
                // Fix: objc2 requires the Vision error out-parameter to be passed explicitly as NSError**.
                msg_send![&*handler, performRequests: &*request_array, onCGImage: &*cg_image, error: &mut error]
            };
            if !success {
                if error.is_null() {
                    return Err("Vision OCR execution failed".to_string());
                }
                let retained_error = unsafe { Retained::retain(error) }
                    .ok_or_else(|| "Vision OCR execution failed".to_string())?;
                return Err(format!(
                    "Vision OCR execution failed: {}",
                    retained_error.localizedDescription()
                ));
            }

            let observations: Option<Retained<NSArray<NSObject>>> =
                unsafe { msg_send![&*request, results] };
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
                .ok_or_else(|| "Vision OCR request class unavailable".to_string())?;
            let handler_cls = AnyClass::get(c"VNSequenceRequestHandler")
                .ok_or_else(|| "Vision OCR handler class unavailable".to_string())?;
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
                        .ok_or_else(|| "--image requires a path argument".to_string())?;
                    image_path = Some(PathBuf::from(value));
                    index += 2;
                }
                "--langs" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--langs requires a value".to_string())?;
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
                    return Err(format!("unsupported argument: {other}"));
                }
            }
        }

        let image_path = image_path.ok_or_else(|| "missing --image argument".to_string())?;
        let text = recognize_image_text(&image_path, &languages)?;
        print!("{}", text);
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod windows_native {
    use super::{default_vision_ocr_languages, parse_lang_list};
    use crate::seekmind::runtime_paths::bundled_vision_ocr_binary_path;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap,
    };
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::StorageFile;
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::System::WinRT::{
        RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED,
    };
    use windows::core::HSTRING;

    pub fn bundled_vision_ocr_binary() -> Option<PathBuf> {
        bundled_vision_ocr_binary_path()
    }

    struct WinRtApartment {
        should_uninitialize: bool,
    }

    impl WinRtApartment {
        fn initialize() -> Result<Self, String> {
            unsafe {
                match RoInitialize(RO_INIT_MULTITHREADED) {
                    Ok(()) => Ok(Self {
                        should_uninitialize: true,
                    }),
                    Err(error) if error.code() == RPC_E_CHANGED_MODE => Ok(Self {
                        should_uninitialize: false,
                    }),
                    Err(error) => Err(format!("Windows OCR runtime init failed: {error}")),
                }
            }
        }
    }

    impl Drop for WinRtApartment {
        fn drop(&mut self) {
            if self.should_uninitialize {
                unsafe {
                    RoUninitialize();
                }
            }
        }
    }

    fn normalize_language_tag(tag: &str) -> String {
        tag.trim().replace('_', "-").to_lowercase()
    }

    fn language_tag(language: &Language) -> Result<String, String> {
        language
            .LanguageTag()
            .map(|value| value.to_string())
            .map_err(|error| format!("Windows OCR language tag read failed: {error}"))
    }

    fn available_languages_internal() -> Result<Vec<Language>, String> {
        let vector = OcrEngine::AvailableRecognizerLanguages()
            .map_err(|error| format!("Windows OCR language probe failed: {error}"))?;
        let size = vector
            .Size()
            .map_err(|error| format!("Windows OCR language vector read failed: {error}"))?;
        let mut languages = Vec::with_capacity(size as usize);
        for index in 0..size {
            let language = vector
                .GetAt(index)
                .map_err(|error| format!("Windows OCR language item read failed: {error}"))?;
            languages.push(language);
        }
        Ok(languages)
    }

    fn language_matches(requested: &str, candidate: &str) -> bool {
        let requested = normalize_language_tag(requested);
        let candidate = normalize_language_tag(candidate);
        if requested == candidate {
            return true;
        }

        let requested_primary = requested.split('-').next().unwrap_or("");
        let candidate_primary = candidate.split('-').next().unwrap_or("");
        !requested_primary.is_empty() && requested_primary == candidate_primary
    }

    fn select_engine(
        requested_languages: &[String],
        available_languages: &[Language],
    ) -> Result<(OcrEngine, String), String> {
        for requested in requested_languages {
            if let Some(language) = available_languages.iter().find(|candidate| {
                language_tag(candidate)
                    .map(|tag| language_matches(requested, &tag))
                    .unwrap_or(false)
            }) {
                let tag = language_tag(language)?;
                let engine = OcrEngine::TryCreateFromLanguage(language)
                    .map_err(|error| format!("Windows OCR engine creation failed for {tag}: {error}"))?;
                eprintln!(
                    "[SeekMind] Windows OCR selected requested language={} resolved={}",
                    requested, tag
                );
                return Ok((engine, tag));
            }
        }

        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .map_err(|error| format!("Windows OCR user-profile engine creation failed: {error}"))?;
        let tag = engine
            .RecognizerLanguage()
            .map_err(|error| format!("Windows OCR recognizer language read failed: {error}"))?
            .LanguageTag()
            .map(|value| value.to_string())
            .map_err(|error| format!("Windows OCR recognizer language tag read failed: {error}"))?;
        eprintln!(
            "[SeekMind] Windows OCR fell back to user profile language={}",
            tag
        );
        Ok((engine, tag))
    }

    fn load_bitmap(path: &Path) -> Result<SoftwareBitmap, String> {
        let hpath = HSTRING::from(path.to_string_lossy().to_string());
        let file = StorageFile::GetFileFromPathAsync(&hpath)
            .map_err(|error| format!("Windows OCR file open request failed: {error}"))?
            .get()
            .map_err(|error| format!("Windows OCR file open failed for {}: {error}", path.display()))?;
        let stream = file
            .OpenReadAsync()
            .map_err(|error| format!("Windows OCR stream open request failed: {error}"))?
            .get()
            .map_err(|error| format!("Windows OCR stream open failed for {}: {error}", path.display()))?;
        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(|error| format!("Windows OCR bitmap decoder request failed: {error}"))?
            .get()
            .map_err(|error| format!("Windows OCR bitmap decoder failed for {}: {error}", path.display()))?;

        match decoder
            .GetSoftwareBitmapConvertedAsync(BitmapPixelFormat::Gray8, BitmapAlphaMode::Ignore)
        {
            Ok(operation) => operation
                .get()
                .map_err(|error| format!("Windows OCR bitmap conversion failed for {}: {error}", path.display())),
            Err(error) => {
                eprintln!(
                    "[SeekMind] Windows OCR gray bitmap conversion unavailable for {}: {}",
                    path.display(),
                    error
                );
                decoder
                    .GetSoftwareBitmapAsync()
                    .map_err(|decode_error| {
                        format!(
                            "Windows OCR bitmap decode request failed for {}: {decode_error}",
                            path.display()
                        )
                    })?
                    .get()
                    .map_err(|decode_error| {
                        format!(
                            "Windows OCR bitmap decode failed for {}: {decode_error}",
                            path.display()
                        )
                    })
            }
        }
    }

    pub fn available_vision_ocr_languages() -> Vec<String> {
        static CACHE: OnceLock<Vec<String>> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                let Ok(_apartment) = WinRtApartment::initialize() else {
                    eprintln!("[SeekMind] Windows OCR runtime initialization failed during probe");
                    return Vec::new();
                };

                let languages = match available_languages_internal() {
                    Ok(languages) => languages,
                    Err(error) => {
                        eprintln!("[SeekMind] Windows OCR language probe failed: {error}");
                        return Vec::new();
                    }
                };

                let tags: Vec<String> = languages
                    .iter()
                    .filter_map(|language| language_tag(language).ok())
                    .collect();
                eprintln!(
                    "[SeekMind] Windows OCR runtime probe succeeded with {} languages",
                    tags.len()
                );
                tags
            })
            .clone()
    }

    pub fn recognize_image_text(path: &Path, languages: &[String]) -> Result<String, String> {
        let _apartment = WinRtApartment::initialize()?;
        let available_languages = available_languages_internal()?;
        if available_languages.is_empty() {
            return Err("Windows OCR has no available recognizer languages".to_string());
        }

        let requested_languages = if languages.is_empty() {
            default_vision_ocr_languages()
        } else {
            languages.to_vec()
        };
        let (engine, resolved_language) = select_engine(&requested_languages, &available_languages)?;
        let bitmap = load_bitmap(path)?;
        let width = bitmap.PixelWidth().unwrap_or_default();
        let height = bitmap.PixelHeight().unwrap_or_default();
        let max_dimension = OcrEngine::MaxImageDimension().unwrap_or_default() as i32;
        if max_dimension > 0 && (width > max_dimension || height > max_dimension) {
            eprintln!(
                "[SeekMind] Windows OCR image exceeds recommended max dimension path={} width={} height={} max={}",
                path.display(),
                width,
                height,
                max_dimension
            );
        }

        let result = engine
            .RecognizeAsync(&bitmap)
            .map_err(|error| format!("Windows OCR request failed for {}: {error}", path.display()))?
            .get()
            .map_err(|error| format!("Windows OCR execution failed for {}: {error}", path.display()))?;
        let text = result
            .Text()
            .map(|value| value.to_string())
            .map_err(|error| format!("Windows OCR text extraction failed for {}: {error}", path.display()))?;
        eprintln!(
            "[SeekMind] Windows OCR completed path={} language={} chars={}",
            path.display(),
            resolved_language,
            text.chars().count()
        );
        Ok(text)
    }

    pub fn run_cli(args: &[String]) -> Result<(), String> {
        if args.iter().any(|arg| arg == "--probe") {
            let languages = available_vision_ocr_languages();
            if languages.is_empty() {
                return Err("Windows OCR probe found no recognizer languages".to_string());
            }
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
                        .ok_or_else(|| "--image requires a path argument".to_string())?;
                    image_path = Some(PathBuf::from(value));
                    index += 2;
                }
                "--langs" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "--langs requires a value".to_string())?;
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
                    return Err(format!("unsupported argument: {other}"));
                }
            }
        }

        let image_path = image_path.ok_or_else(|| "missing --image argument".to_string())?;
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

#[cfg(target_os = "windows")]
#[allow(unused_imports)]
pub use windows_native::{
    available_vision_ocr_languages, bundled_vision_ocr_binary, recognize_image_text, run_cli,
};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::path::{Path, PathBuf};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn bundled_vision_ocr_binary() -> Option<PathBuf> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn available_vision_ocr_languages() -> Vec<String> {
    Vec::new()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn recognize_image_text(_path: &Path, _languages: &[String]) -> Result<String, String> {
    Err("OCR runtime is only available on macOS and Windows".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn run_cli(_args: &[String]) -> Result<(), String> {
    Err("OCR runtime is only available on macOS and Windows".to_string())
}
