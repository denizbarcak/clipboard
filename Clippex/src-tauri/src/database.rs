use crate::models::{ClipboardItem, ClipboardItemType, Collection};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_CLIPBOARD_ITEMS: u32 = 20;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Veritabanını aç veya oluştur
    pub fn new(app_data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Veri dizini oluşturulamadı: {}", e))?;

        let db_path = app_data_dir.join("clipboard.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Veritabanı açılamadı: {}", e))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clipboard_items (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                item_type TEXT NOT NULL DEFAULT 'text',
                source_app TEXT,
                preview TEXT,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_created_at ON clipboard_items(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_is_pinned ON clipboard_items(is_pinned);
            CREATE INDEX IF NOT EXISTS idx_item_type ON clipboard_items(item_type);

            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '#6c5ce7',
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS collection_items (
                collection_id TEXT NOT NULL,
                item_id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                item_type TEXT NOT NULL DEFAULT 'text',
                preview TEXT,
                added_at TEXT NOT NULL,
                FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_collection_id ON collection_items(collection_id);",
        )
        .map_err(|e| format!("Tablo oluşturulamadı: {}", e))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // ===== Clipboard Items =====

    /// Yeni öğe ekle + eski öğeleri temizle (limit aşılırsa)
    pub fn insert_item(&self, item: &ClipboardItem) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO clipboard_items (id, content, item_type, source_app, preview, is_pinned, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                item.id,
                item.content,
                item.item_type.as_str(),
                item.source_app,
                item.preview,
                item.is_pinned as i32,
                item.created_at,
            ],
        )
        .map_err(|e| format!("Öğe eklenemedi: {}", e))?;

        // 20 limitini aşan eski öğeleri sil (pinlenmiş olanlar hariç)
        conn.execute(
            "DELETE FROM clipboard_items WHERE is_pinned = 0 AND id NOT IN (
                SELECT id FROM clipboard_items ORDER BY is_pinned DESC, created_at DESC LIMIT ?1
            )",
            params![MAX_CLIPBOARD_ITEMS],
        )
        .map_err(|e| format!("Eski öğeler temizlenemedi: {}", e))?;

        Ok(())
    }

    pub fn get_items(&self, limit: u32, offset: u32) -> Result<Vec<ClipboardItem>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT ci.id, ci.content, ci.item_type, ci.source_app, ci.preview, ci.is_pinned, ci.created_at, c.color
                 FROM clipboard_items ci
                 LEFT JOIN collection_items coi ON ci.id = coi.item_id
                 LEFT JOIN collections c ON coi.collection_id = c.id
                 ORDER BY ci.is_pinned DESC, ci.created_at DESC
                 LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| format!("Sorgu hazırlanamadı: {}", e))?;

        let items = stmt
            .query_map(params![limit, offset], |row| {
                Ok(ClipboardItem {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    item_type: ClipboardItemType::from_str(
                        &row.get::<_, String>(2).unwrap_or_default(),
                    ),
                    source_app: row.get(3)?,
                    preview: row.get(4)?,
                    is_pinned: row.get::<_, i32>(5).unwrap_or(0) != 0,
                    created_at: row.get(6)?,
                    collection_color: row.get(7)?,
                })
            })
            .map_err(|e| format!("Sorgu çalıştırılamadı: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    pub fn search_items(&self, query: &str) -> Result<Vec<ClipboardItem>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let search = format!("%{}%", query);
        let mut stmt = conn
            .prepare(
                "SELECT ci.id, ci.content, ci.item_type, ci.source_app, ci.preview, ci.is_pinned, ci.created_at, c.color
                 FROM clipboard_items ci
                 LEFT JOIN collection_items coi ON ci.id = coi.item_id
                 LEFT JOIN collections c ON coi.collection_id = c.id
                 WHERE ci.content LIKE ?1
                 ORDER BY ci.is_pinned DESC, ci.created_at DESC
                 LIMIT 50",
            )
            .map_err(|e| format!("Arama sorgusu hazırlanamadı: {}", e))?;

        let items = stmt
            .query_map(params![search], |row| {
                Ok(ClipboardItem {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    item_type: ClipboardItemType::from_str(
                        &row.get::<_, String>(2).unwrap_or_default(),
                    ),
                    source_app: row.get(3)?,
                    preview: row.get(4)?,
                    is_pinned: row.get::<_, i32>(5).unwrap_or(0) != 0,
                    created_at: row.get(6)?,
                    collection_color: row.get(7)?,
                })
            })
            .map_err(|e| format!("Arama çalıştırılamadı: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    pub fn update_item_content(&self, id: &str, content: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE clipboard_items SET content = ?1 WHERE id = ?2",
            params![content, id],
        )
        .map_err(|e| format!("İçerik güncellenemedi: {}", e))?;
        Ok(())
    }

    pub fn update_collection_item_content(&self, id: &str, content: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE collection_items SET content = ?1 WHERE item_id = ?2",
            params![content, id],
        )
        .map_err(|e| format!("Koleksiyon içeriği güncellenemedi: {}", e))?;
        Ok(())
    }

    /// Hem clipboard hem koleksiyon öğelerinde arama, opsiyonel renk filtresi
    pub fn search_all(&self, query: &str, color: Option<&str>) -> Result<Vec<ClipboardItem>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let search = format!("%{}%", query);

        if let Some(color) = color {
            let mut stmt = conn.prepare(
                "SELECT coi.item_id, coi.content, coi.item_type, NULL, coi.preview, 0, coi.added_at, c.color
                 FROM collection_items coi
                 JOIN collections c ON coi.collection_id = c.id
                 WHERE coi.content LIKE ?1 AND c.color = ?2
                 ORDER BY coi.added_at DESC
                 LIMIT 50"
            ).map_err(|e| format!("Arama hazırlanamadı: {}", e))?;

            let items = stmt.query_map(params![search, color], |row| {
                Ok(ClipboardItem {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    item_type: ClipboardItemType::from_str(&row.get::<_, String>(2).unwrap_or_default()),
                    source_app: row.get(3)?,
                    preview: row.get(4)?,
                    is_pinned: false,
                    created_at: row.get(6)?,
                    collection_color: row.get(7)?,
                })
            }).map_err(|e| format!("Arama çalıştırılamadı: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

            return Ok(items);
        } else {
            // Clipboard öğelerinde ara
            let mut stmt = conn.prepare(
                "SELECT ci.id, ci.content, ci.item_type, ci.source_app, ci.preview, ci.is_pinned, ci.created_at, c.color
                 FROM clipboard_items ci
                 LEFT JOIN collection_items coi ON ci.id = coi.item_id
                 LEFT JOIN collections c ON coi.collection_id = c.id
                 WHERE ci.content LIKE ?1
                 ORDER BY ci.created_at DESC
                 LIMIT 50"
            ).map_err(|e| format!("Arama hazırlanamadı: {}", e))?;

            let items = stmt.query_map(params![search], |row| {
                Ok(ClipboardItem {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    item_type: ClipboardItemType::from_str(&row.get::<_, String>(2).unwrap_or_default()),
                    source_app: row.get(3)?,
                    preview: row.get(4)?,
                    is_pinned: row.get::<_, i32>(5).unwrap_or(0) != 0,
                    created_at: row.get(6)?,
                    collection_color: row.get(7)?,
                })
            }).map_err(|e| format!("Arama çalıştırılamadı: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

            return Ok(items);
        };
    }

    pub fn delete_item(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])
            .map_err(|e| format!("Öğe silinemedi: {}", e))?;
        Ok(())
    }

    pub fn toggle_pin(&self, id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE clipboard_items SET is_pinned = CASE WHEN is_pinned = 0 THEN 1 ELSE 0 END WHERE id = ?1",
            params![id],
        )
        .map_err(|e| format!("Pin güncellenemedi: {}", e))?;

        let pinned: bool = conn
            .query_row(
                "SELECT is_pinned FROM clipboard_items WHERE id = ?1",
                params![id],
                |row| row.get::<_, i32>(0).map(|v| v != 0),
            )
            .map_err(|e| format!("Pin durumu okunamadı: {}", e))?;

        Ok(pinned)
    }

    pub fn clear_history(&self) -> Result<u64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let deleted = conn
            .execute("DELETE FROM clipboard_items WHERE is_pinned = 0", [])
            .map_err(|e| format!("Geçmiş temizlenemedi: {}", e))?;
        Ok(deleted as u64)
    }

    pub fn get_last_content(&self) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let result = conn.query_row(
            "SELECT content FROM clipboard_items ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(content) => Ok(Some(content)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Son içerik okunamadı: {}", e)),
        }
    }

    // ===== Collections =====

    pub fn create_collection(&self, collection: &Collection) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO collections (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![collection.id, collection.name, collection.color, collection.created_at],
        )
        .map_err(|e| format!("Koleksiyon oluşturulamadı: {}", e))?;
        Ok(())
    }

    pub fn get_collections(&self) -> Result<Vec<Collection>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare("SELECT id, name, color, created_at FROM collections ORDER BY created_at ASC")
            .map_err(|e| format!("Koleksiyon sorgusu hazırlanamadı: {}", e))?;

        let collections = stmt
            .query_map([], |row| {
                Ok(Collection {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Koleksiyonlar alınamadı: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(collections)
    }

    pub fn delete_collection(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM collection_items WHERE collection_id = ?1", params![id])
            .map_err(|e| format!("Koleksiyon öğeleri silinemedi: {}", e))?;
        conn.execute("DELETE FROM collections WHERE id = ?1", params![id])
            .map_err(|e| format!("Koleksiyon silinemedi: {}", e))?;
        Ok(())
    }

    pub fn rename_collection(&self, id: &str, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE collections SET name = ?1 WHERE id = ?2",
            params![name, id],
        )
        .map_err(|e| format!("Koleksiyon adı güncellenemedi: {}", e))?;
        Ok(())
    }

    pub fn update_collection_color(&self, id: &str, color: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE collections SET color = ?1 WHERE id = ?2",
            params![color, id],
        )
        .map_err(|e| format!("Koleksiyon rengi güncellenemedi: {}", e))?;
        Ok(())
    }

    // ===== Collection Items =====

    pub fn add_to_collection(
        &self,
        collection_id: &str,
        item_id: &str,
        content: &str,
        item_type: &str,
        preview: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR REPLACE INTO collection_items (collection_id, item_id, content, item_type, preview, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            params![collection_id, item_id, content, item_type, preview],
        )
        .map_err(|e| format!("Koleksiyona eklenemedi: {}", e))?;
        Ok(())
    }

    pub fn get_collection_items(&self, collection_id: &str) -> Result<Vec<ClipboardItem>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT coi.item_id, coi.content, coi.item_type, coi.preview, coi.added_at, c.color
                 FROM collection_items coi
                 JOIN collections c ON coi.collection_id = c.id
                 WHERE coi.collection_id = ?1
                 ORDER BY coi.added_at DESC",
            )
            .map_err(|e| format!("Koleksiyon öğeleri sorgusu hazırlanamadı: {}", e))?;

        let items = stmt
            .query_map(params![collection_id], |row| {
                Ok(ClipboardItem {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    item_type: ClipboardItemType::from_str(
                        &row.get::<_, String>(2).unwrap_or_default(),
                    ),
                    source_app: None,
                    preview: row.get(3)?,
                    is_pinned: false,
                    created_at: row.get(4)?,
                    collection_color: row.get(5)?,
                })
            })
            .map_err(|e| format!("Koleksiyon öğeleri alınamadı: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    pub fn get_collection_item_count(&self, collection_id: &str) -> Result<u32, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM collection_items WHERE collection_id = ?1",
                params![collection_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Sayım yapılamadı: {}", e))?;
        Ok(count)
    }

    pub fn remove_from_collection(&self, item_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM collection_items WHERE item_id = ?1",
            params![item_id],
        )
        .map_err(|e| format!("Koleksiyondan çıkarılamadı: {}", e))?;
        Ok(())
    }
}
