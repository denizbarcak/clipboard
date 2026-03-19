use crate::database::Database;
use crate::models::{ClipboardItem, ClipboardItemType};
use arboard::Clipboard;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

/// Clipboard'ı arka planda izle ve değişiklikleri yakala
pub fn start_clipboard_watcher(app_handle: AppHandle, db: Arc<Database>) {
    std::thread::spawn(move || {
        let mut clipboard = match Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                log::error!("Clipboard başlatılamadı: {}", e);
                return;
            }
        };

        let mut last_text: Option<String> = None;

        loop {
            std::thread::sleep(Duration::from_millis(500));

            // Metin kontrolü
            if let Ok(text) = clipboard.get_text() {
                let text = text.trim().to_string();
                if !text.is_empty() && !text.starts_with("webview-panel\\") {
                    let is_new = match &last_text {
                        Some(last) => last != &text,
                        None => {
                            // İlk çalıştırmada DB'deki son içerikle karşılaştır
                            match db.get_last_content() {
                                Ok(Some(last_db)) => last_db != text,
                                _ => true,
                            }
                        }
                    };

                    if is_new {
                        last_text = Some(text.clone());
                    }
                    if is_new && !crate::IS_PASTING.load(std::sync::atomic::Ordering::SeqCst) {
                        let item_type = detect_content_type(&text);
                        let preview = create_preview(&text, &item_type);

                        let item = ClipboardItem {
                            id: Uuid::new_v4().to_string(),
                            content: text,
                            item_type,
                            source_app: None,
                            preview,
                            is_pinned: false,
                            created_at: Utc::now().to_rfc3339(),
                            collection_color: None,
                            title: None,
                        };

                        if let Err(e) = db.insert_item(&item) {
                            log::error!("Clipboard öğesi kaydedilemedi: {}", e);
                        } else {
                            // Frontend'e bildir
                            let _ = app_handle.emit("clipboard-changed", &item);
                            log::info!("Yeni clipboard öğesi kaydedildi: {}", item.id);

                            // Sync: sunucuya gönder (async, bloklamaz)
                            let sync = app_handle.state::<crate::sync::SyncManager>();
                            if sync.is_logged_in() {
                                let content = item.content.clone();
                                let item_type = format!("{:?}", item.item_type).to_lowercase();
                                let state = sync.get_state();
                                std::thread::spawn(move || {
                                    let rt = tokio::runtime::Builder::new_current_thread()
                                        .enable_all()
                                        .build()
                                        .unwrap();
                                    rt.block_on(async {
                                        let client = reqwest::Client::new();
                                        let token = state.token.unwrap_or_default();
                                        let body = serde_json::json!({
                                            "content": content,
                                            "item_type": item_type,
                                            "device_id": state.device_id.unwrap_or(0),
                                        });
                                        match client
                                            .post(format!("{}/sync", state.api_url))
                                            .header("Authorization", format!("Bearer {}", token))
                                            .json(&body)
                                            .send()
                                            .await
                                        {
                                            Ok(_) => log::info!("Clipboard synced: {} chars", content.len()),
                                            Err(e) => log::warn!("Sync push hatası: {}", e),
                                        }
                                    });
                                });
                            }
                        }
                    }
                }
            }

            // TODO: Faz 2'de resim desteği eklenecek
        }
    });
}

/// İçerik türünü otomatik algıla
fn detect_content_type(text: &str) -> ClipboardItemType {
    let trimmed = text.trim();

    // URL kontrolü
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("www.")
    {
        return ClipboardItemType::Link;
    }

    // Dosya yolu kontrolü (Windows)
    if (trimmed.len() >= 3 && trimmed.chars().nth(1) == Some(':') && trimmed.chars().nth(2) == Some('\\'))
        || trimmed.starts_with("\\\\")
    {
        return ClipboardItemType::FilePath;
    }

    // Dosya yolu kontrolü (macOS / Linux)
    if trimmed.starts_with("/Users/")
        || trimmed.starts_with("/home/")
        || trimmed.starts_with("/tmp/")
        || trimmed.starts_with("/var/")
        || trimmed.starts_with("/opt/")
        || trimmed.starts_with("~/")
    {
        return ClipboardItemType::FilePath;
    }

    ClipboardItemType::Text
}

/// Kart önizlemesi için kısa metin oluştur
fn create_preview(text: &str, item_type: &ClipboardItemType) -> Option<String> {
    match item_type {
        ClipboardItemType::Link => {
            // URL'den domain çıkar
            if let Some(start) = text.find("://") {
                let after = &text[start + 3..];
                let domain = after.split('/').next().unwrap_or(after);
                Some(domain.to_string())
            } else {
                Some(text.chars().take(50).collect())
            }
        }
        ClipboardItemType::FilePath => {
            // Dosya adını çıkar
            let name = text.rsplit(['\\', '/']).next().unwrap_or(text);
            Some(name.to_string())
        }
        ClipboardItemType::Text => {
            if text.len() > 100 {
                Some(format!("{}...", &text.chars().take(100).collect::<String>()))
            } else {
                None // Kısa metinlerde preview gereksiz
            }
        }
        ClipboardItemType::Image => Some("Resim".to_string()),
    }
}
