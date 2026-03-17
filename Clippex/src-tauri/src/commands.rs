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
    sync: State<'_, crate::sync::SyncManager>,
    id: String,
    content: String,
    is_collection_item: bool,
) -> Result<(), String> {
    let result = if is_collection_item {
        state.0.update_collection_item_content(&id, &content)
    } else {
        state.0.update_item_content(&id, &content)
    };

    // Sync: düzenlemeyi sunucuya bildir (full sync ile güncellenecek)
    if result.is_ok() {
        sync.send_or_queue("edit_item", serde_json::json!({
            "id": id,
            "content": content,
            "is_collection_item": is_collection_item,
        }));
    }

    result
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
    sync: State<'_, crate::sync::SyncManager>,
    name: String,
    color: Option<String>,
) -> Result<Collection, String> {
    let collection = Collection {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.clone(),
        color: color.clone().unwrap_or_else(|| "#6c5ce7".to_string()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.0.create_collection(&collection)?;

    sync.send_or_queue("create_collection", serde_json::json!({
        "local_id": collection.id,
        "name": collection.name,
        "color": collection.color,
    }));

    Ok(collection)
}

#[tauri::command]
pub fn get_collections(state: State<'_, DbState>) -> Result<Vec<Collection>, String> {
    state.0.get_collections()
}

#[tauri::command]
pub fn delete_collection(
    state: State<'_, DbState>,
    sync: State<'_, crate::sync::SyncManager>,
    id: String,
) -> Result<(), String> {
    state.0.delete_collection(&id)?;
    sync.send_or_queue("delete_collection", serde_json::json!({ "local_id": id }));
    Ok(())
}

#[tauri::command]
pub fn rename_collection(
    state: State<'_, DbState>,
    sync: State<'_, crate::sync::SyncManager>,
    id: String,
    name: String,
) -> Result<(), String> {
    state.0.rename_collection(&id, &name)?;
    sync.send_or_queue("rename_collection", serde_json::json!({ "local_id": id, "name": name }));
    Ok(())
}

#[tauri::command]
pub fn update_collection_color(
    state: State<'_, DbState>,
    sync: State<'_, crate::sync::SyncManager>,
    id: String,
    color: String,
) -> Result<(), String> {
    state.0.update_collection_color(&id, &color)?;
    sync.send_or_queue("update_color", serde_json::json!({ "local_id": id, "color": color }));
    Ok(())
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
    sync: State<'_, crate::sync::SyncManager>,
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
    )?;

    sync.send_or_queue("add_to_collection", serde_json::json!({
        "local_id": collection_id,
        "content": content,
        "item_type": item_type,
    }));

    Ok(())
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
pub fn toggle_autostart(
    app: tauri::AppHandle,
    state: State<'_, SettingsState>,
    enabled: bool,
) -> Result<(), String> {
    use tauri_plugin_autostart::AutoLaunchManager;
    use tauri::Manager;

    let autostart = app.state::<AutoLaunchManager>();
    if enabled {
        autostart.enable().map_err(|e| format!("Autostart etkinleştirilemedi: {}", e))?;
    } else {
        autostart.disable().map_err(|e| format!("Autostart devre dışı bırakılamadı: {}", e))?;
    }
    state.0.set_autostart(enabled)?;
    log::info!("Autostart: {}", enabled);
    Ok(())
}

#[tauri::command]
pub fn set_dragging(dragging: bool) {
    crate::IS_DRAGGING.store(dragging, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
pub fn remove_from_collection(
    state: State<'_, DbState>,
    sync: State<'_, crate::sync::SyncManager>,
    item_id: String,
    collection_id: Option<String>,
) -> Result<(), String> {
    // Öğenin içeriğini al (sunucuya göndermek için)
    if let Some(ref col_id) = collection_id {
        if let Ok(items) = state.0.get_collection_items(col_id) {
            if let Some(item) = items.iter().find(|i| i.id == item_id) {
                sync.send_or_queue("remove_from_collection", serde_json::json!({
                    "local_id": col_id,
                    "content": item.content,
                }));
            }
        }
    }
    state.0.remove_from_collection(&item_id)
}

// ===== SYNC COMMANDS =====

#[tauri::command]
pub async fn sync_login(
    sync: State<'_, crate::sync::SyncManager>,
    db: State<'_, DbState>,
    token: String,
    email: String,
) -> Result<String, String> {
    sync.set_auth(token.clone(), email.clone(), None);

    // Register device
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".to_string());
    match sync.register_device(&hostname).await {
        Ok(device_id) => {
            log::info!("Sync giriş yapıldı: {} (device: {})", email, device_id);
        }
        Err(e) => {
            log::warn!("Device kaydedilemedi: {}", e);
        }
    }

    let state = sync.get_state();
    let client = reqwest::Client::new();
    let auth_header = format!("Bearer {}", state.token.clone().unwrap_or_default());
    let device_id = state.device_id.unwrap_or(0);

    // Full sync: ana pano + koleksiyonlar tek seferde
    let clipboard_items = db.0.get_items(20, 0).unwrap_or_default();
    let items_data: Vec<serde_json::Value> = clipboard_items.iter().map(|item| {
        serde_json::json!({
            "content": item.content,
            "item_type": format!("{:?}", item.item_type).to_lowercase(),
        })
    }).collect();

    let collections = db.0.get_collections().unwrap_or_default();
    let collections_data: Vec<serde_json::Value> = collections.iter().map(|col| {
        let items = db.0.get_collection_items(&col.id).unwrap_or_default();
        let item_data: Vec<serde_json::Value> = items.iter().map(|item| {
            serde_json::json!({
                "content": item.content,
                "item_type": format!("{:?}", item.item_type).to_lowercase(),
            })
        }).collect();

        serde_json::json!({
            "id": col.id,
            "name": col.name,
            "color": col.color,
            "items": item_data,
        })
    }).collect();

    let body = serde_json::json!({
        "clipboard_items": items_data,
        "collections": collections_data,
        "device_id": device_id,
    });

    match client
        .post(format!("{}/sync/full", state.api_url))
        .header("Authorization", &auth_header)
        .json(&body)
        .send()
        .await
    {
        Ok(res) => {
            if res.status().is_success() {
                log::info!("Full sync tamamlandı: {} kart, {} koleksiyon", items_data.len(), collections_data.len());
            } else {
                log::warn!("Full sync başarısız: {}", res.status());
            }
        }
        Err(e) => log::warn!("Full sync hatası: {}", e),
    }

    Ok("Giriş yapıldı, veriler senkronize edildi".to_string())
}

#[tauri::command]
pub fn sync_logout(sync: State<'_, crate::sync::SyncManager>) -> Result<(), String> {
    sync.clear_auth();
    log::info!("Sync çıkış yapıldı");
    Ok(())
}

#[tauri::command]
pub fn sync_status(sync: State<'_, crate::sync::SyncManager>) -> Result<bool, String> {
    Ok(sync.is_logged_in())
}
