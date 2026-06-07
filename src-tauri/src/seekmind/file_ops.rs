use std::path::Path;

pub fn open_file_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("文件路径不能为空".to_string());
    }

    if !Path::new(path).exists() {
        return Err(format!("文件不存在: {path}"));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path])
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
mod macos_quicklook {
    use std::cell::RefCell;
    use std::ffi::{c_char, CString};
    use std::ptr::NonNull;

    use objc2::__framework_prelude::*;
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2::MainThreadMarker;
    use objc2::{define_class, msg_send, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSBackingStoreType, NSEvent, NSFloatingWindowLevel, NSPanel, NSResponder,
        NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSURL};

    #[link(name = "QuickLookUI", kind = "framework")]
    extern "C" {}

    extern_class!(
        /// Quick Look preview view embedded in an AppKit window.
        #[unsafe(super(NSView, NSResponder, NSObject))]
        pub struct QLPreviewView;
    );

    #[allow(non_snake_case)]
    impl QLPreviewView {
        extern_methods!(
            #[unsafe(method(initWithFrame:style:))]
            #[unsafe(method_family = init)]
            pub unsafe fn initWithFrame_style(
                this: Allocated<Self>,
                frame: NSRect,
                style: usize,
            ) -> Retained<Self>;

            #[unsafe(method(setPreviewItem:))]
            #[unsafe(method_family = none)]
            pub fn setPreviewItem(&self, preview_item: Option<&AnyObject>);

            #[unsafe(method(refreshPreviewItem))]
            #[unsafe(method_family = none)]
            pub fn refreshPreviewItem(&self);
        );
    }

    define_class!(
        #[unsafe(super(QLPreviewView))]
        #[name = "SeekMindQuickLookPreviewView"]
        #[thread_kind = MainThreadOnly]
        struct SeekMindQuickLookPreviewView;

        unsafe impl NSObjectProtocol for SeekMindQuickLookPreviewView {}

        impl SeekMindQuickLookPreviewView {
            #[unsafe(method(acceptsFirstResponder))]
            fn accepts_first_responder(&self) -> bool {
                true
            }

            #[unsafe(method(keyDown:))]
            fn key_down(&self, event: &NSEvent) {
                if event.keyCode() == 53 {
                    let window: Option<Retained<NSPanel>> = unsafe { msg_send![self, window] };
                    if let Some(window) = window {
                        // Closing a QLPreviewView deactivates its internal state. Reusing that
                        // view later will abort inside QuickLook, so ESC only hides the panel.
                        window.orderOut(None);
                    }
                    return;
                }

                unsafe {
                    let _: () = msg_send![super(self), keyDown: event];
                }
            }
        }
    );

    impl SeekMindQuickLookPreviewView {
        fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
            let this = mtm.alloc();
            unsafe { msg_send![this, initWithFrame: frame, style: 0usize] }
        }
    }

    struct QuickLookPanelState {
        panel: Retained<NSPanel>,
        preview_view: Retained<SeekMindQuickLookPreviewView>,
        current_url: Option<Retained<NSURL>>,
        current_path: Option<String>,
    }

    thread_local! {
        static QUICK_LOOK_PANEL: RefCell<Option<QuickLookPanelState>> = RefCell::new(None);
    }

    fn make_preview_frame() -> NSRect {
        // Keep the preview closer to a document-reading aspect ratio so the canvas
        // does not leave a large unused gutter on the right.
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(700.0, 780.0))
    }

    unsafe fn create_panel_state(mtm: MainThreadMarker) -> QuickLookPanelState {
        let frame = make_preview_frame();
        let style =
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable;

        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        );

        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(false);
        panel.setWorksWhenModal(true);
        panel.setHidesOnDeactivate(false);
        panel.setLevel(NSFloatingWindowLevel);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::Transient | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setMovableByWindowBackground(true);
        panel.setReleasedWhenClosed(false);
        panel.center();

        let preview_view = SeekMindQuickLookPreviewView::new(mtm, frame);
        let preview_view_nsview: &NSView = &*(Retained::as_ptr(&preview_view) as *const NSView);
        preview_view_nsview.setAutoresizingMask(
            objc2_app_kit::NSAutoresizingMaskOptions::ViewWidthSizable
                | objc2_app_kit::NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        panel.setContentView(Some(preview_view_nsview));
        let preview_view_responder: &NSResponder =
            &*(Retained::as_ptr(&preview_view) as *const NSResponder);
        panel.makeFirstResponder(Some(preview_view_responder));

        QuickLookPanelState {
            panel,
            preview_view,
            current_url: None,
            current_path: None,
        }
    }

    pub fn show_quick_look_preview(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
        let path = path.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || {
            let mtm = MainThreadMarker::new().expect("Quick Look must run on main thread");
            let app_ref = NSApplication::sharedApplication(mtm);
            app_ref.activate();

            let result = QUICK_LOOK_PANEL.with(|cell| {
                let mut state = cell.borrow_mut();
                if state.is_none() {
                    *state = Some(unsafe { create_panel_state(mtm) });
                }

                // If the user closed the AppKit panel with the close button, QuickLook deactivates
                // the embedded QLPreviewView. Do not call setPreviewItem on that stale view again.
                if state
                    .as_ref()
                    .is_some_and(|value| value.current_path.is_some() && !value.panel.isVisible())
                {
                    *state = Some(unsafe { create_panel_state(mtm) });
                }

                if let Some(state) = state.as_mut() {
                    if state.current_path.as_deref() == Some(path.as_str())
                        && state.panel.isVisible()
                    {
                        state.panel.makeKeyAndOrderFront(None);
                        state.panel.orderFrontRegardless();
                        return Ok::<(), String>(());
                    }

                    let c_path = CString::new(path.clone()).map_err(|error| error.to_string())?;
                    let ns_path = unsafe {
                        NSString::initWithUTF8String(
                            NSString::alloc(),
                            NonNull::new(c_path.as_ptr() as *mut c_char)
                                .ok_or_else(|| "Quick Look 路径转换失败".to_string())?,
                        )
                        .ok_or_else(|| "Quick Look 路径转换失败".to_string())?
                    };
                    let file_url = NSURL::fileURLWithPath(&ns_path);
                    let preview_item: &AnyObject =
                        unsafe { &*(Retained::as_ptr(&file_url) as *const AnyObject) };
                    state.current_url = Some(file_url);
                    state.current_path = Some(path.clone());
                    state.preview_view.setPreviewItem(Some(preview_item));
                    state.preview_view.refreshPreviewItem();
                    state.panel.makeKeyAndOrderFront(None);
                    state.panel.orderFrontRegardless();
                    state.panel.center();
                }

                Ok::<(), String>(())
            });

            let _ = tx.send(result);
        })
        .map_err(|error| error.to_string())?;

        rx.recv()
            .map_err(|_| "Quick Look 面板打开失败".to_string())?
    }
}

#[cfg(target_os = "macos")]
pub fn quick_look_file_path(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("文件路径不能为空".to_string());
    }

    if !Path::new(path).exists() {
        return Err(format!("文件不存在: {path}"));
    }

    macos_quicklook::show_quick_look_preview(app, path)
}

#[cfg(not(target_os = "macos"))]
pub fn quick_look_file_path(_app: &tauri::AppHandle, _path: &str) -> Result<(), String> {
    Err("Quick Look 仅在 macOS 上可用".to_string())
}
