use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub shortcut: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: "ctrl+alt+v".to_string(),
        }
    }
}

pub struct SettingsManager {
    path: PathBuf,
    settings: Mutex<AppSettings>,
}

impl SettingsManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let path = app_data_dir.join("settings.json");
        let settings = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => AppSettings::default(),
            }
        } else {
            AppSettings::default()
        };

        let mgr = Self {
            path,
            settings: Mutex::new(settings),
        };
        mgr.save().ok();
        mgr
    }

    pub fn get(&self) -> AppSettings {
        self.settings.lock().unwrap().clone()
    }

    pub fn set_shortcut(&self, shortcut: &str) -> Result<(), String> {
        {
            let mut s = self.settings.lock().map_err(|e| e.to_string())?;
            s.shortcut = shortcut.to_string();
        }
        self.save()
    }

    fn save(&self) -> Result<(), String> {
        let s = self.settings.lock().map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(&*s)
            .map_err(|e| format!("Ayarlar serialize edilemedi: {}", e))?;
        std::fs::write(&self.path, json)
            .map_err(|e| format!("Ayarlar kaydedilemedi: {}", e))?;
        Ok(())
    }
}
