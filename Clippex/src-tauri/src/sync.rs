use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub token: Option<String>,
    pub api_url: String,
    pub device_id: Option<i64>,
    pub user_email: Option<String>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            token: None,
            api_url: "http://localhost:3456/api".to_string(),
            device_id: None,
            user_email: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub action: String,     // "push_clipboard", "create_collection", "delete_collection", "add_to_collection", "remove_from_collection", "rename_collection", "update_color"
    pub payload: serde_json::Value,
}

pub struct SyncManager {
    state: Mutex<SyncState>,
    queue: Mutex<Vec<QueueItem>>,
    client: reqwest::Client,
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SyncState::default()),
            queue: Mutex::new(Vec::new()),
            client: reqwest::Client::new(),
        }
    }

    pub fn set_auth(&self, token: String, email: String, device_id: Option<i64>) {
        let mut state = self.state.lock().unwrap();
        state.token = Some(token);
        state.user_email = Some(email);
        state.device_id = device_id;
    }

    pub fn clear_auth(&self) {
        let mut state = self.state.lock().unwrap();
        state.token = None;
        state.user_email = None;
        state.device_id = None;
    }

    pub fn is_logged_in(&self) -> bool {
        self.state.lock().unwrap().token.is_some()
    }

    pub fn get_state(&self) -> SyncState {
        self.state.lock().unwrap().clone()
    }

    /// Add action to offline queue
    pub fn enqueue(&self, action: &str, payload: serde_json::Value) {
        self.queue.lock().unwrap().push(QueueItem {
            action: action.to_string(),
            payload,
        });
    }

    /// Get and clear the queue
    pub fn drain_queue(&self) -> Vec<QueueItem> {
        let mut queue = self.queue.lock().unwrap();
        let items: Vec<QueueItem> = queue.drain(..).collect();
        items
    }

    /// Process a single sync action
    pub async fn process_action(&self, action: &str, payload: &serde_json::Value) -> Result<(), String> {
        let state = self.state.lock().unwrap().clone();
        let token = match &state.token {
            Some(t) => t.clone(),
            None => return Ok(()),
        };
        let auth = format!("Bearer {}", token);

        match action {
            "push_clipboard" => {
                self.client
                    .post(format!("{}/sync", state.api_url))
                    .header("Authorization", &auth)
                    .json(payload)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
            }
            "create_collection" => {
                self.client
                    .post(format!("{}/collections", state.api_url))
                    .header("Authorization", &auth)
                    .json(payload)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
            }
            "delete_collection" => {
                let local_id = payload["local_id"].as_str().unwrap_or("");
                self.client
                    .delete(format!("{}/collections/local/{}", state.api_url, local_id))
                    .header("Authorization", &auth)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
            }
            "rename_collection" => {
                let local_id = payload["local_id"].as_str().unwrap_or("");
                self.client
                    .put(format!("{}/collections/local/{}", state.api_url, local_id))
                    .header("Authorization", &auth)
                    .json(payload)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
            }
            "update_color" => {
                let local_id = payload["local_id"].as_str().unwrap_or("");
                self.client
                    .put(format!("{}/collections/local/{}", state.api_url, local_id))
                    .header("Authorization", &auth)
                    .json(payload)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
            }
            "add_to_collection" => {
                let local_id = payload["local_id"].as_str().unwrap_or("");
                // Find server collection by local_id, then add item
                self.client
                    .post(format!("{}/collections", state.api_url))
                    .header("Authorization", &auth)
                    .json(&serde_json::json!({
                        "local_id": local_id,
                        "content": payload["content"],
                        "item_type": payload["item_type"],
                    }))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
            }
            "remove_from_collection" => {
                let local_id = payload["local_id"].as_str().unwrap_or("");
                self.client
                    .delete(format!("{}/collections/local/{}/items", state.api_url, local_id))
                    .header("Authorization", &auth)
                    .json(payload)
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Try to send action, if fails enqueue it
    pub fn send_or_queue(&self, action: &str, payload: serde_json::Value) {
        if !self.is_logged_in() {
            return;
        }

        let action = action.to_string();
        let payload_clone = payload.clone();
        let state = self.get_state();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let client = reqwest::Client::new();
                let token = state.token.unwrap_or_default();
                let auth = format!("Bearer {}", token);

                let result = match action.as_str() {
                    "push_clipboard" => {
                        client
                            .post(format!("{}/sync", state.api_url))
                            .header("Authorization", &auth)
                            .json(&payload_clone)
                            .send()
                            .await
                    }
                    "create_collection" => {
                        client
                            .post(format!("{}/collections", state.api_url))
                            .header("Authorization", &auth)
                            .json(&payload_clone)
                            .send()
                            .await
                    }
                    "delete_collection" => {
                        let local_id = payload_clone["local_id"].as_str().unwrap_or("");
                        client
                            .delete(format!("{}/collections/local/{}", state.api_url, local_id))
                            .header("Authorization", &auth)
                            .send()
                            .await
                    }
                    "rename_collection" | "update_color" => {
                        let local_id = payload_clone["local_id"].as_str().unwrap_or("");
                        client
                            .put(format!("{}/collections/local/{}", state.api_url, local_id))
                            .header("Authorization", &auth)
                            .json(&payload_clone)
                            .send()
                            .await
                    }
                    "add_to_collection" => {
                        let local_id = payload_clone["local_id"].as_str().unwrap_or("");
                        client
                            .post(format!("{}/collections/local/{}/items", state.api_url, local_id))
                            .header("Authorization", &auth)
                            .json(&payload_clone)
                            .send()
                            .await
                    }
                    "remove_from_collection" => {
                        let local_id = payload_clone["local_id"].as_str().unwrap_or("");
                        client
                            .delete(format!("{}/collections/local/{}/items", state.api_url, local_id))
                            .header("Authorization", &auth)
                            .json(&payload_clone)
                            .send()
                            .await
                    }
                    "edit_sync_item" => {
                        client
                            .put(format!("{}/sync/item", state.api_url))
                            .header("Authorization", &auth)
                            .json(&payload_clone)
                            .send()
                            .await
                    }
                    "edit_collection_item" => {
                        let local_id = payload_clone["local_id"].as_str().unwrap_or("");
                        client
                            .put(format!("{}/collections/local/{}/items", state.api_url, local_id))
                            .header("Authorization", &auth)
                            .json(&payload_clone)
                            .send()
                            .await
                    }
                    _ => return,
                };

                match result {
                    Ok(res) => {
                        if res.status().is_success() {
                            log::info!("Sync {}: başarılı", action);
                        } else {
                            log::warn!("Sync {} başarısız: {}", action, res.status());
                        }
                    }
                    Err(e) => {
                        log::warn!("Sync {} hatası, kuyruğa alındı: {}", action, e);
                        // Offline queue'ya kaydet
                        let queue_item = serde_json::json!({
                            "action": action,
                            "payload": payload_clone,
                        });
                        let queue_dir = dirs::data_local_dir()
                            .unwrap_or_default()
                            .join("clippex");
                        let _ = std::fs::create_dir_all(&queue_dir);
                        let queue_file = queue_dir.join("offline_queue.json");
                        let mut queue: Vec<serde_json::Value> = std::fs::read_to_string(&queue_file)
                            .ok()
                            .and_then(|s| serde_json::from_str(&s).ok())
                            .unwrap_or_default();
                        queue.push(queue_item);
                        let _ = std::fs::write(&queue_file, serde_json::to_string(&queue).unwrap_or_default());
                    }
                }
            });
        });
    }

    /// Register this device with the server
    pub async fn register_device(&self, device_name: &str) -> Result<i64, String> {
        let state = self.state.lock().unwrap().clone();
        let token = match &state.token {
            Some(t) => t.clone(),
            None => return Err("Giriş yapılmamış".to_string()),
        };

        let body = serde_json::json!({
            "device_name": device_name,
            "device_type": "desktop",
        });

        let res = self.client
            .post(format!("{}/devices", state.api_url))
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Device kayıt hatası: {}", e))?;

        if !res.status().is_success() {
            return Err("Device kaydedilemedi".to_string());
        }

        let data: DeviceResponse = res.json().await.map_err(|e| format!("Parse hatası: {}", e))?;
        self.state.lock().unwrap().device_id = Some(data.device.id);

        log::info!("Device kaydedildi: id={}", data.device.id);
        Ok(data.device.id)
    }
}

#[derive(Debug, Deserialize)]
pub struct SyncItem {
    pub id: i64,
    pub content: String,
    pub item_type: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
struct DeviceResponse {
    device: DeviceInfo,
}

#[derive(Debug, Deserialize)]
struct DeviceInfo {
    id: i64,
}
