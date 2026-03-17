use serde::{Deserialize, Serialize};

/// Clipboard öğesinin türü
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClipboardItemType {
    Text,
    Image,
    Link,
    FilePath,
}

impl ClipboardItemType {
    pub fn as_str(&self) -> &str {
        match self {
            ClipboardItemType::Text => "text",
            ClipboardItemType::Image => "image",
            ClipboardItemType::Link => "link",
            ClipboardItemType::FilePath => "filepath",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "image" => ClipboardItemType::Image,
            "link" => ClipboardItemType::Link,
            "filepath" => ClipboardItemType::FilePath,
            _ => ClipboardItemType::Text,
        }
    }
}

/// Clipboard'dan alınan her öğe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: String,
    pub content: String,
    pub item_type: ClipboardItemType,
    pub source_app: Option<String>,
    pub preview: Option<String>,
    pub is_pinned: bool,
    pub created_at: String,
    pub collection_color: Option<String>,
    pub title: Option<String>,
}

/// Koleksiyon (dosyalama)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
}
