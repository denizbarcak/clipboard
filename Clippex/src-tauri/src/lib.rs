mod clipboard;
mod commands;
mod database;
mod models;
mod settings;
mod sync;

use commands::DbState;
use database::Database;
use settings::SettingsManager;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// Panel açılmadan önce aktif olan pencere handle'ı (Windows)
static PREVIOUS_WINDOW: AtomicIsize = AtomicIsize::new(0);
/// macOS: Önceki aktif uygulamanın adı
#[cfg(target_os = "macos")]
static PREVIOUS_APP: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
/// Sürükleme sırasında pencere kapanmasını engelle
pub static IS_DRAGGING: AtomicBool = AtomicBool::new(false);
pub static IS_PASTING: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // İkinci instance açılmaya çalışınca mevcut pencereyi göster
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            // macOS: Dock'ta gösterme — sadece system tray'de çalış
            #[cfg(target_os = "macos")]
            {
                extern "C" {
                    fn objc_getClass(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
                    fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
                    fn objc_msgSend(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, ...) -> *mut std::ffi::c_void;
                }
                unsafe {
                    let cls = objc_getClass(b"NSApplication\0".as_ptr() as *const _);
                    let shared = sel_registerName(b"sharedApplication\0".as_ptr() as *const _);
                    let app = objc_msgSend(cls, shared);
                    let set_policy = sel_registerName(b"setActivationPolicy:\0".as_ptr() as *const _);
                    // NSApplicationActivationPolicyAccessory = 1
                    let _: *mut std::ffi::c_void = {
                        let f: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i64) -> *mut std::ffi::c_void = std::mem::transmute(objc_msgSend as *const ());
                        f(app, set_policy, 1)
                    };
                }
                log::info!("macOS: Dock'ta gizlendi (Accessory mode)");
            }

            // Logger (her zaman aktif — sorun tespiti için)
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .build(),
            )?;

            // Veritabanı başlat
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("App data dizini bulunamadı");

            let db = Arc::new(
                Database::new(app_data_dir.clone()).expect("Veritabanı başlatılamadı"),
            );

            let settings = Arc::new(
                SettingsManager::new(app_data_dir),
            );

            // State olarak kaydet (Tauri commands için)
            app.manage(DbState(db.clone()));
            app.manage(commands::SettingsState(settings.clone()));

            // Sync manager
            let sync_manager = sync::SyncManager::new();
            app.manage(sync_manager);

            // Clipboard watcher başlat
            clipboard::start_clipboard_watcher(app.handle().clone(), db);

            // Pencereyi konumlandır
            if let Some(window) = app.get_webview_window("main") {
                position_window(&window);

                // Platform'a göre blur efekti
                apply_window_effects(&window);

                // macOS: Pencere level'ını Dock üzerine çıkar
                #[cfg(target_os = "macos")]
                {
                    set_macos_window_level(&window);
                }

                // Focus kaybedince pencereyi gizle (sürükleme sırasında değilse)
                let w = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        if !IS_DRAGGING.load(Ordering::SeqCst) {
                            let _ = w.hide();
                        }
                    }
                });
            }

            // System tray
            setup_tray(app)?;

            // Global shortcut
            let shortcut_str = settings.get().shortcut;
            setup_global_shortcut(app, &shortcut_str)?;

            log::info!("Clippex başlatıldı");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_clipboard_items,
            commands::search_clipboard,
            commands::search_all,
            commands::update_clipboard_content,
            commands::delete_clipboard_item,
            commands::toggle_pin_item,
            commands::clear_clipboard_history,
            commands::copy_to_clipboard,
            commands::hide_window,
            commands::paste_to_previous_window,
            commands::create_collection,
            commands::get_collections,
            commands::delete_collection,
            commands::rename_collection,
            commands::get_collection_item_count,
            commands::add_item_to_collection,
            commands::get_collection_items,
            commands::remove_from_collection,
            commands::reorder_collection_items,
            commands::update_collection_color,
            commands::set_dragging,
            commands::get_settings,
            commands::update_shortcut,
            commands::toggle_autostart,
            commands::sync_login,
            commands::sync_logout,
            commands::sync_status,
        ])
        .run(tauri::generate_context!())
        .expect("Uygulama başlatılırken hata oluştu");
}

// ===== Pencere Konumlandırma =====

/// Pencereyi ekranın altına konumlandır (platform bağımsız)
fn position_window(window: &tauri::WebviewWindow) {
    use tauri::PhysicalPosition;

    let cursor_pos = get_cursor_position();
    let target_monitor = if let Ok(monitors) = window.available_monitors() {
        monitors.into_iter().find(|m| {
            let pos = m.position();
            let size = m.size();
            cursor_pos.0 >= pos.x
                && cursor_pos.0 < pos.x + size.width as i32
                && cursor_pos.1 >= pos.y
                && cursor_pos.1 < pos.y + size.height as i32
        })
    } else {
        None
    };

    let monitor = target_monitor.or_else(|| window.primary_monitor().ok().flatten());

    if let Some(monitor) = monitor {
        let screen_size = monitor.size();
        let screen_pos = monitor.position();

        let panel_width = screen_size.width as i32;

        // macOS: Biraz daha yüksek panel
        #[cfg(target_os = "macos")]
        let ratio = if screen_size.height <= 1080 { 0.26 }
            else if screen_size.height <= 1920 { 0.28 }
            else { 0.26 };
        #[cfg(not(target_os = "macos"))]
        let ratio = if screen_size.height <= 1080 { 0.22 }
            else if screen_size.height <= 1920 { 0.25 }
            else { 0.22 };

        let panel_height = (screen_size.height as f64 * ratio) as i32;

        let x = screen_pos.x;

        // macOS: Ekranın en altına yapıştır
        #[cfg(target_os = "macos")]
        let y = {
            screen_pos.y + screen_size.height as i32 - panel_height
        };
        #[cfg(not(target_os = "macos"))]
        let y = screen_pos.y + screen_size.height as i32 - panel_height;

        // Windows: DPI geçişi için önce ortaya taşı, bekle, sonra pozisyonla
        #[cfg(target_os = "windows")]
        {
            let _ = window.set_position(PhysicalPosition::new(
                screen_pos.x + screen_size.width as i32 / 2,
                screen_pos.y + screen_size.height as i32 / 2,
            ));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        // Boyut ve pozisyon ayarla
        let _ = window.set_size(tauri::PhysicalSize::new(panel_width as u32, panel_height as u32));
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// Platform'a göre pencere blur efekti uygula
fn apply_window_effects(window: &tauri::WebviewWindow) {
    use tauri::window::{Effect, EffectState};

    #[cfg(target_os = "windows")]
    {
        let _ = window.set_effects(tauri::utils::config::WindowEffectsConfig {
            effects: vec![Effect::Acrylic],
            ..Default::default()
        });
    }

    #[cfg(target_os = "macos")]
    {
        let _ = window.set_effects(tauri::utils::config::WindowEffectsConfig {
            effects: vec![Effect::HudWindow],
            state: Some(EffectState::Active),
            radius: Some(12.0),
            ..Default::default()
        });
    }
}

// ===== Pencere Üste Zorlama =====

/// Windows API: Pencereyi en üste zorla (Start menüsünün bile üstüne)
fn force_topmost(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        const HWND_TOPMOST: isize = -1;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_SHOWWINDOW: u32 = 0x0040;

        extern "system" {
            fn SetWindowPos(
                hwnd: isize,
                hwnd_insert_after: isize,
                x: i32,
                y: i32,
                cx: i32,
                cy: i32,
                flags: u32,
            ) -> i32;
        }

        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                SetWindowPos(
                    hwnd.0 as isize,
                    HWND_TOPMOST,
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        set_macos_window_level(window);
    }
}

// ===== macOS Pencere Level (Dock üzerinde) =====

#[cfg(target_os = "macos")]
fn set_macos_window_level(window: &tauri::WebviewWindow) {
    use tauri::Listener;
    // NSWindow setLevel: ile pencereyi Dock üzerine çıkar
    // Dock level = 20, biz 24 (floating) veya 25 kullanıyoruz
    let ns_window = window.ns_window();
    if let Ok(ns_win) = ns_window {
        unsafe {
            let sel = sel_register_name(b"setLevel:\0");
            let f: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i64) = std::mem::transmute(objc_msg_send_fn() as *const ());
            f(ns_win as *mut std::ffi::c_void, sel, 25); // 25 = kCGScreenSaverWindowLevel üzeri
        }
        log::info!("macOS: Pencere level 25 ayarlandı (Dock üzerinde)");
    }
}

#[cfg(target_os = "macos")]
unsafe fn sel_register_name(name: &[u8]) -> *mut std::ffi::c_void {
    extern "C" {
        fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    }
    sel_registerName(name.as_ptr() as *const _)
}

#[cfg(target_os = "macos")]
fn objc_msg_send_fn() -> unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, ...) -> *mut std::ffi::c_void {
    extern "C" {
        fn objc_msgSend(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, ...) -> *mut std::ffi::c_void;
    }
    objc_msgSend
}

// ===== macOS Dock Yüksekliği =====

#[cfg(target_os = "macos")]
fn get_macos_dock_height(screen_physical_height: u32, scale_factor: f64) -> i32 {
    // AppleScript ile Finder desktop bounds alarak visible area hesapla
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"Finder\" to get bounds of window of desktop")
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Çıktı: "0, 0, 1710, 1112" formatında — son değer visible height (logical)
        let parts: Vec<&str> = stdout.trim().split(", ").collect();
        if parts.len() == 4 {
            if let (Ok(menu_y), Ok(visible_bottom)) = (parts[1].parse::<f64>(), parts[3].parse::<f64>()) {
                let logical_screen_height = screen_physical_height as f64 / scale_factor;
                let dock_height = logical_screen_height - visible_bottom;
                return (dock_height * scale_factor) as i32;
            }
        }
    }
    // Fallback: tipik Dock yüksekliği
    (70.0 * scale_factor) as i32
}

// ===== Cursor Pozisyonu =====

/// Cursor pozisyonunu al (platform bağımsız)
fn get_cursor_position() -> (i32, i32) {
    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        struct POINT {
            x: i32,
            y: i32,
        }
        extern "system" {
            fn GetCursorPos(point: *mut POINT) -> i32;
        }
        let mut point = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut point) };
        return (point.x, point.y);
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: CoreGraphics ile mouse pozisyonu al
        extern "C" {
            fn CGEventCreate(source: *const std::ffi::c_void) -> *const std::ffi::c_void;
            fn CGEventGetLocation(event: *const std::ffi::c_void) -> CGPoint;
            fn CFRelease(cf: *const std::ffi::c_void);
        }

        #[repr(C)]
        struct CGPoint {
            x: f64,
            y: f64,
        }

        unsafe {
            let event = CGEventCreate(std::ptr::null());
            if !event.is_null() {
                let point = CGEventGetLocation(event);
                CFRelease(event);
                return (point.x as i32, point.y as i32);
            }
        }
        return (0, 0);
    }

    #[allow(unreachable_code)]
    (0, 0)
}

// ===== Pencere Toggle =====

/// Pencereyi göster/gizle toggle
fn toggle_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            // macOS: Önceki uygulamayı SENKRON kaydet (panel açılmadan önce!)
            #[cfg(target_os = "macos")]
            {
                save_previous_foreground_window();
            }
            // Windows: Senkron kaydet (hızlı API çağrısı)
            #[cfg(not(target_os = "macos"))]
            {
                save_previous_foreground_window();
            }

            let _ = window.hide();
            position_window(&window);
            // Windows: Pozisyonlama sonrası kısa bekleme
            #[cfg(not(target_os = "macos"))]
            std::thread::sleep(std::time::Duration::from_millis(10));
            let _ = window.show();
            let _ = window.set_focus();
            force_topmost(&window);
            // Kısa bekle ve tekrar üste zorla (Windows Start menüsü vb. için)
            let w = window.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                force_topmost(&w);
                let _ = w.set_focus();
            });
            let _ = window.emit("window-shown", ());
        }
    }
}

// ===== Önceki Pencere Yönetimi =====

/// Aktif pencereyi kaydet
fn save_previous_foreground_window() {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn GetForegroundWindow() -> isize;
        }
        let hwnd = unsafe { GetForegroundWindow() };
        PREVIOUS_WINDOW.store(hwnd, Ordering::SeqCst);
        log::info!("Önceki pencere kaydedildi: {}", hwnd);
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: Önceki aktif uygulamanın adını AppleScript ile al
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "System Events" to get name of first application process whose frontmost is true"#)
            .output();

        if let Ok(o) = output {
            if o.status.success() {
                let app_name = String::from_utf8_lossy(&o.stdout).trim().to_string();
                log::info!("macOS: Önceki uygulama kaydedildi: {}", app_name);
                if let Ok(mut prev) = PREVIOUS_APP.lock() {
                    *prev = Some(app_name);
                }
            }
        }
    }
}

/// Önceki pencereye focus ver ve yapıştır (Ctrl+V / Cmd+V)
pub fn restore_and_paste() {
    #[cfg(target_os = "windows")]
    {
        use std::mem;

        const INPUT_KEYBOARD: u32 = 1;
        const KEYEVENTF_KEYUP: u32 = 0x0002;
        const VK_CONTROL: u16 = 0x11;
        const VK_V: u16 = 0x56;

        #[repr(C)]
        struct KEYBDINPUT {
            w_vk: u16,
            w_scan: u16,
            dw_flags: u32,
            time: u32,
            dw_extra_info: usize,
        }

        #[repr(C)]
        struct INPUT {
            input_type: u32,
            ki: KEYBDINPUT,
            _padding: [u8; 8], // Union padding
        }

        extern "system" {
            fn SetForegroundWindow(hwnd: isize) -> i32;
            fn SendInput(c_inputs: u32, p_inputs: *const INPUT, cb_size: i32) -> u32;
        }

        let hwnd = PREVIOUS_WINDOW.load(Ordering::SeqCst);
        if hwnd == 0 {
            log::warn!("Önceki pencere bulunamadı");
            return;
        }

        // Önceki pencereye focus ver
        unsafe { SetForegroundWindow(hwnd) };

        // Kısa bekleme - pencerenin focus alması için
        std::thread::sleep(std::time::Duration::from_millis(80));

        // Ctrl+V simüle et
        let inputs = [
            // Ctrl basılı
            INPUT {
                input_type: INPUT_KEYBOARD,
                ki: KEYBDINPUT {
                    w_vk: VK_CONTROL,
                    w_scan: 0,
                    dw_flags: 0,
                    time: 0,
                    dw_extra_info: 0,
                },
                _padding: [0u8; 8],
            },
            // V basılı
            INPUT {
                input_type: INPUT_KEYBOARD,
                ki: KEYBDINPUT {
                    w_vk: VK_V,
                    w_scan: 0,
                    dw_flags: 0,
                    time: 0,
                    dw_extra_info: 0,
                },
                _padding: [0u8; 8],
            },
            // V bırakıldı
            INPUT {
                input_type: INPUT_KEYBOARD,
                ki: KEYBDINPUT {
                    w_vk: VK_V,
                    w_scan: 0,
                    dw_flags: KEYEVENTF_KEYUP,
                    time: 0,
                    dw_extra_info: 0,
                },
                _padding: [0u8; 8],
            },
            // Ctrl bırakıldı
            INPUT {
                input_type: INPUT_KEYBOARD,
                ki: KEYBDINPUT {
                    w_vk: VK_CONTROL,
                    w_scan: 0,
                    dw_flags: KEYEVENTF_KEYUP,
                    time: 0,
                    dw_extra_info: 0,
                },
                _padding: [0u8; 8],
            },
        ];

        let sent = unsafe {
            SendInput(
                4,
                inputs.as_ptr(),
                mem::size_of::<INPUT>() as i32,
            )
        };

        log::info!("Ctrl+V simüle edildi, {} input gönderildi", sent);
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: Önceki uygulamaya dön ve Cmd+V simüle et
        let app_name = PREVIOUS_APP.lock().ok().and_then(|p| p.clone());

        if let Some(app) = app_name {
            // Önceki uygulamayı öne getir ve Cmd+V gönder
            let script = format!(
                r#"tell application "{}" to activate
delay 0.05
tell application "System Events" to keystroke "v" using command down"#,
                app
            );

            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    log::info!("macOS: {} uygulamasına Cmd+V gönderildi", app);
                }
                Ok(o) => {
                    log::warn!("macOS: AppleScript hatası: {}", String::from_utf8_lossy(&o.stderr));
                }
                Err(e) => {
                    log::warn!("macOS: osascript çalıştırılamadı: {}", e);
                }
            }
        } else {
            log::warn!("macOS: Önceki uygulama bulunamadı");
        }
    }
}

/// Global kısayol kaydet
fn setup_global_shortcut(app: &tauri::App, shortcut_str: &str) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

    let shortcut: Shortcut = shortcut_str.parse().map_err(|e| format!("{:?}", e))?;
    let handle = app.handle().clone();

    let _ = app.global_shortcut().unregister(shortcut);

    match app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
            toggle_window(&handle);
        }
    }) {
        Ok(_) => log::info!("Global kısayol kaydedildi: {}", shortcut_str),
        Err(e) => log::warn!("Global kısayol kaydedilemedi: {}", e),
    }

    Ok(())
}

/// System tray ikonu ve menüsü
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::TrayIconBuilder;

    let show = MenuItemBuilder::with_id("show", "Göster").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Çıkış").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&quit)
        .build()?;

    let handle = app.handle().clone();

    let icon = tauri::image::Image::from_path("icons/icon.png")
        .or_else(|_| tauri::image::Image::from_path("icons/32x32.png"))
        .unwrap_or_else(|_| app.default_window_icon().unwrap().clone());

    TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("Clippex")
        .on_menu_event(move |_app, event| match event.id().as_ref() {
            "show" => {
                toggle_window(&handle);
            }
            "quit" => {
                std::process::exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
