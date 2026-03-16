mod clipboard;
mod commands;
mod database;
mod models;
mod settings;

use commands::DbState;
use database::Database;
use settings::SettingsManager;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// Panel açılmadan önce aktif olan pencere handle'ı
static PREVIOUS_WINDOW: AtomicIsize = AtomicIsize::new(0);
/// Sürükleme sırasında pencere kapanmasını engelle
pub static IS_DRAGGING: AtomicBool = AtomicBool::new(false);

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
            // Logger (debug modda)
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

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

            // Clipboard watcher başlat
            clipboard::start_clipboard_watcher(app.handle().clone(), db);

            // Pencereyi görev çubuğunun hemen üstüne konumlandır
            if let Some(window) = app.get_webview_window("main") {
                position_window_above_taskbar(&window);

                // Acrylic/Mica blur efekti (Windows 10/11)
                use tauri::window::Effect;
                let _ = window.set_effects(tauri::utils::config::WindowEffectsConfig {
                    effects: vec![Effect::Acrylic],
                    ..Default::default()
                });

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

            log::info!("Clipboard Manager başlatıldı");
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
            commands::update_collection_color,
            commands::set_dragging,
            commands::get_settings,
            commands::update_shortcut,
            commands::toggle_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("Uygulama başlatılırken hata oluştu");
}

/// Pencereyi cursor'un bulunduğu monitörün altına yapıştır
fn position_window_above_taskbar(window: &tauri::WebviewWindow) {
    use tauri::PhysicalPosition;

    // Cursor hangi monitörde?
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

    // Bulunamazsa primary monitor'ü kullan
    let monitor = target_monitor.or_else(|| window.primary_monitor().ok().flatten());

    if let Some(monitor) = monitor {
        let screen_size = monitor.size();
        let screen_pos = monitor.position();

        let panel_width = screen_size.width as i32;
        // Küçük ekranlarda biraz daha yüksek, büyük ekranlarda daha düşük
        let ratio = if screen_size.height <= 1080 { 0.22 }
            else if screen_size.height <= 1920 { 0.25 }
            else { 0.22 };
        let panel_height = (screen_size.height as f64 * ratio) as i32;

        let x = screen_pos.x;
        let y = screen_pos.y + screen_size.height as i32 - panel_height;

        // 1) Önce pencereyi hedef monitörün ortasına taşı (DPI geçişi için)
        let _ = window.set_position(PhysicalPosition::new(
            screen_pos.x + screen_size.width as i32 / 2,
            screen_pos.y + screen_size.height as i32 / 2,
        ));
        // 2) Kısa bekle — DPI değişimi uygulansın
        std::thread::sleep(std::time::Duration::from_millis(20));
        // 3) Şimdi doğru boyut ve pozisyon
        let _ = window.set_size(tauri::PhysicalSize::new(panel_width as u32, panel_height as u32));
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

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
}

/// Windows API: Cursor pozisyonunu al
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
    #[allow(unreachable_code)]
    (0, 0)
}

/// Pencereyi göster/gizle toggle
fn toggle_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            // Panel açılmadan önce aktif pencereyi kaydet
            save_previous_foreground_window();
            let _ = window.hide();
            position_window_above_taskbar(&window);
            std::thread::sleep(std::time::Duration::from_millis(10));
            let _ = window.show();
            let _ = window.set_focus();
            // Windows API ile en üste zorla
            force_topmost(&window);
            // Start menüsü kapanması için bekle ve tekrar üste zorla
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

/// Aktif pencereyi kaydet (Windows API)
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
}

/// Önceki pencereye focus ver ve Ctrl+V simüle et
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
        Ok(_) => log::info!("Global kısayol kaydedildi: Ctrl+Shift+V"),
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
        .tooltip("Clipboard Manager")
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
