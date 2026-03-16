use crate::database::Database;
use crate::models::{ClipboardItem, Collection};
use crate::settings::{AppSettings, SettingsManager};
use std::sync::Arc;
use tauri::State;

pub struct DbState(pub Arc<Database>);
pub struct SettingsState(pub Arc<SettingsManager>);

// ===== Clipboard =====

#[tauri::command]
pub fn get_clipboard_items(
    state: State<'_, DbState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<ClipboardItem>, String> {
    state.0.get_items(limit.unwrap_or(50), offset.unwrap_or(0))
}

#[tauri::command]
pub fn search_clipboard(
    state: State<'_, DbState>,
    query: String,
) -> Result<Vec<ClipboardItem>, String> {
    state.0.search_items(&query)
}

#[tauri::command]
pub fn update_clipboard_content(
    state: State<'_, DbState>,
    id: String,
    content: String,
    is_collection_item: bool,
) -> Result<(), String> {
    if is_collection_item {
        state.0.update_collection_item_content(&id, &content)
    } else {
        state.0.update_item_content(&id, &content)
    }
}

#[tauri::command]
pub fn search_all(
    state: State<'_, DbState>,
    query: String,
    color: Option<String>,
) -> Result<Vec<ClipboardItem>, String> {
    state.0.search_all(&query, color.as_deref())
}

#[tauri::command]
pub fn delete_clipboard_item(state: State<'_, DbState>, id: String) -> Result<(), String> {
    state.0.delete_item(&id)
}

#[tauri::command]
pub fn toggle_pin_item(state: State<'_, DbState>, id: String) -> Result<bool, String> {
    state.0.toggle_pin(&id)
}

#[tauri::command]
pub fn clear_clipboard_history(state: State<'_, DbState>) -> Result<u64, String> {
    state.0.clear_history()
}

#[tauri::command]
pub fn hide_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_to_clipboard(content: String) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard açılamadı: {}", e))?;
    clipboard
        .set_text(&content)
        .map_err(|e| format!("Clipboard'a yazılamadı: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn paste_to_previous_window(window: tauri::WebviewWindow, content: String) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard açılamadı: {}", e))?;
    clipboard
        .set_text(&content)
        .map_err(|e| format!("Clipboard'a yazılamadı: {}", e))?;
    let _ = window.hide();
    crate::restore_and_paste();
    Ok(())
}

// ===== Collections =====

#[tauri::command]
pub fn create_collection(
    state: State<'_, DbState>,
    name: String,
    color: Option<String>,
) -> Result<Collection, String> {
    let collection = Collection {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        color: color.unwrap_or_else(|| "#6c5ce7".to_string()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.0.create_collection(&collection)?;
    Ok(collection)
}

#[tauri::command]
pub fn get_collections(state: State<'_, DbState>) -> Result<Vec<Collection>, String> {
    state.0.get_collections()
}

#[tauri::command]
pub fn delete_collection(state: State<'_, DbState>, id: String) -> Result<(), String> {
    state.0.delete_collection(&id)
}

#[tauri::command]
pub fn rename_collection(
    state: State<'_, DbState>,
    id: String,
    name: String,
) -> Result<(), String> {
    state.0.rename_collection(&id, &name)
}

#[tauri::command]
pub fn update_collection_color(
    state: State<'_, DbState>,
    id: String,
    color: String,
) -> Result<(), String> {
    state.0.update_collection_color(&id, &color)
}

#[tauri::command]
pub fn get_collection_item_count(
    state: State<'_, DbState>,
    collection_id: String,
) -> Result<u32, String> {
    state.0.get_collection_item_count(&collection_id)
}

#[tauri::command]
pub fn add_item_to_collection(
    state: State<'_, DbState>,
    collection_id: String,
    item_id: String,
    content: String,
    item_type: String,
    preview: Option<String>,
) -> Result<(), String> {
    state.0.add_to_collection(
        &collection_id,
        &item_id,
        &content,
        &item_type,
        preview.as_deref(),
    )
}

#[tauri::command]
pub fn get_collection_items(
    state: State<'_, DbState>,
    collection_id: String,
) -> Result<Vec<ClipboardItem>, String> {
    state.0.get_collection_items(&collection_id)
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Result<AppSettings, String> {
    Ok(state.0.get())
}

#[tauri::command]
pub fn update_shortcut(
    app: tauri::AppHandle,
    state: State<'_, SettingsState>,
    shortcut: String,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

    // Yeni kısayolu parse et
    let new_shortcut: Shortcut = shortcut.parse().map_err(|e| format!("Geçersiz kısayol: {:?}", e))?;

    // Eski kısayolu kaldır
    let old_shortcut_str = state.0.get().shortcut;
    if let Ok(old_shortcut) = old_shortcut_str.parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(old_shortcut);
    }

    // Yeni kısayolu kaydet
    let handle = app.clone();
    let _ = app.global_shortcut().unregister(new_shortcut);
    app.global_shortcut().on_shortcut(new_shortcut, move |_app, _shortcut, event| {
        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
            crate::toggle_window(&handle);
        }
    }).map_err(|e| format!("Kısayol kaydedilemedi: {}", e))?;

    // Ayarlara kaydet
    state.0.set_shortcut(&shortcut)?;
    log::info!("Kısayol güncellendi: {}", shortcut);
    Ok(())
}

#[tauri::command]
pub fn set_dragging(dragging: bool) {
    crate::IS_DRAGGING.store(dragging, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
pub fn remove_from_collection(
    state: State<'_, DbState>,
    item_id: String,
) -> Result<(), String> {
    state.0.remove_from_collection(&item_id)
}
