const express = require("express");
const bcrypt = require("bcryptjs");
const db = require("./db");
const { generateToken, authMiddleware } = require("./auth");

const router = express.Router();

// ===== AUTH =====

// Register
router.post("/auth/register", async (req, res) => {
  const { email, password } = req.body;

  if (!email || !password) {
    return res.status(400).json({ error: "E-posta ve şifre gerekli" });
  }

  if (password.length < 6) {
    return res.status(400).json({ error: "Şifre en az 6 karakter olmalı" });
  }

  const existing = db.prepare("SELECT id FROM users WHERE email = ?").get(email);
  if (existing) {
    return res.status(409).json({ error: "Bu e-posta zaten kayıtlı" });
  }

  const hashedPassword = await bcrypt.hash(password, 10);
  const result = db.prepare("INSERT INTO users (email, password) VALUES (?, ?)").run(email, hashedPassword);

  const user = { id: result.lastInsertRowid, email };
  const token = generateToken(user);

  res.status(201).json({
    message: "Kayıt başarılı",
    token,
    user: { id: user.id, email },
  });
});

// Login
router.post("/auth/login", async (req, res) => {
  const { email, password } = req.body;

  if (!email || !password) {
    return res.status(400).json({ error: "E-posta ve şifre gerekli" });
  }

  const user = db.prepare("SELECT * FROM users WHERE email = ?").get(email);
  if (!user) {
    return res.status(401).json({ error: "E-posta veya şifre hatalı" });
  }

  const valid = await bcrypt.compare(password, user.password);
  if (!valid) {
    return res.status(401).json({ error: "E-posta veya şifre hatalı" });
  }

  const token = generateToken(user);

  res.json({
    message: "Giriş başarılı",
    token,
    user: { id: user.id, email: user.email },
  });
});

// Get current user
router.get("/auth/me", authMiddleware, (req, res) => {
  const user = db.prepare("SELECT id, email, created_at FROM users WHERE id = ?").get(req.user.id);
  if (!user) {
    return res.status(404).json({ error: "Kullanıcı bulunamadı" });
  }
  res.json({ user });
});

// ===== DEVICES =====

// Register device
router.post("/devices", authMiddleware, (req, res) => {
  const { device_name, device_type } = req.body;
  const name = device_name || "Windows PC";
  const type = device_type || "desktop";

  // Aynı cihaz varsa mevcut kaydı döndür
  const existing = db.prepare(
    "SELECT * FROM devices WHERE user_id = ? AND device_name = ?"
  ).get(req.user.id, name);

  if (existing) {
    db.prepare("UPDATE devices SET last_seen = datetime('now') WHERE id = ?").run(existing.id);
    return res.status(200).json({
      device: { id: existing.id, device_name: existing.device_name, device_type: existing.device_type },
    });
  }

  const result = db.prepare(
    "INSERT INTO devices (user_id, device_name, device_type) VALUES (?, ?, ?)"
  ).run(req.user.id, name, type);

  res.status(201).json({
    device: { id: result.lastInsertRowid, device_name: name, device_type: type },
  });
});

// List devices
router.get("/devices", authMiddleware, (req, res) => {
  const devices = db.prepare(
    "SELECT id, device_name, device_type, last_seen, created_at FROM devices WHERE user_id = ?"
  ).all(req.user.id);

  res.json({ devices });
});

// ===== SYNC =====

// Push clipboard item
router.post("/sync", authMiddleware, (req, res) => {
  const { content, item_type, device_id } = req.body;

  if (!content) {
    return res.status(400).json({ error: "İçerik gerekli" });
  }

  const result = db.prepare(
    "INSERT INTO sync_items (user_id, device_id, content, item_type) VALUES (?, ?, ?, ?)"
  ).run(req.user.id, device_id || 0, content, item_type || "text");

  // 20 limit: fazla olanları sil
  const count = db.prepare("SELECT COUNT(*) as c FROM sync_items WHERE user_id = ?").get(req.user.id);
  if (count.c > 20) {
    db.prepare(`
      DELETE FROM sync_items WHERE id IN (
        SELECT id FROM sync_items WHERE user_id = ? ORDER BY created_at ASC LIMIT ?
      )
    `).run(req.user.id, count.c - 20);
  }

  // Update device last_seen
  if (device_id) {
    db.prepare("UPDATE devices SET last_seen = CURRENT_TIMESTAMP WHERE id = ?").run(device_id);
  }

  res.status(201).json({
    item: {
      id: result.lastInsertRowid,
      content,
      item_type: item_type || "text",
    },
  });
});

// Clear sync items (ana pano sıfırlama)
router.delete("/sync", authMiddleware, (req, res) => {
  db.prepare("DELETE FROM sync_items WHERE user_id = ?").run(req.user.id);
  res.json({ message: "Sync items temizlendi" });
});

// Get sync items (from other devices)
router.get("/sync", authMiddleware, (req, res) => {
  const { since, device_id } = req.query;

  let query = "SELECT * FROM sync_items WHERE user_id = ?";
  const params = [req.user.id];

  // Exclude items from current device
  if (device_id) {
    query += " AND device_id != ?";
    params.push(device_id);
  }

  // Only items after a certain time
  if (since) {
    query += " AND created_at > ?";
    params.push(since);
  }

  query += " ORDER BY created_at DESC LIMIT 50";

  const items = db.prepare(query).all(...params);
  res.json({ items });
});

// ===== COLLECTIONS =====

// Initial sync — upload all local collections + items on login
router.post("/sync/initial", authMiddleware, (req, res) => {
  const { collections } = req.body;

  if (!collections || !Array.isArray(collections)) {
    return res.status(400).json({ error: "collections array gerekli" });
  }

  const insertCollection = db.prepare(
    "INSERT INTO collections (user_id, local_id, name, color) VALUES (?, ?, ?, ?)"
  );
  const insertItem = db.prepare(
    "INSERT INTO collection_items (collection_id, content, item_type) VALUES (?, ?, ?)"
  );

  const syncResult = db.transaction(() => {
    // Mevcut koleksiyonları temizle (fresh sync)
    const existingCollections = db.prepare("SELECT id FROM collections WHERE user_id = ?").all(req.user.id);
    for (const col of existingCollections) {
      db.prepare("DELETE FROM collection_items WHERE collection_id = ?").run(col.id);
    }
    db.prepare("DELETE FROM collections WHERE user_id = ?").run(req.user.id);

    let syncedCollections = 0;
    let syncedItems = 0;

    for (const col of collections) {
      const result = insertCollection.run(req.user.id, col.id || null, col.name, col.color || "#6c5ce7");
      syncedCollections++;

      if (col.items && Array.isArray(col.items)) {
        for (const item of col.items) {
          insertItem.run(result.lastInsertRowid, item.content, item.item_type || "text");
          syncedItems++;
        }
      }
    }

    return { syncedCollections, syncedItems };
  })();

  res.json({
    message: "İlk senkronizasyon tamamlandı",
    ...syncResult,
  });
});

// Get all collections with items
router.get("/collections", authMiddleware, (req, res) => {
  const collections = db.prepare(
    "SELECT * FROM collections WHERE user_id = ? ORDER BY created_at"
  ).all(req.user.id);

  const result = collections.map((col) => {
    const items = db.prepare(
      "SELECT * FROM collection_items WHERE collection_id = ? ORDER BY created_at DESC"
    ).all(col.id);
    return { ...col, items };
  });

  res.json({ collections: result });
});

// Create collection
router.post("/collections", authMiddleware, (req, res) => {
  const { name, color, local_id } = req.body;

  const result = db.prepare(
    "INSERT INTO collections (user_id, local_id, name, color) VALUES (?, ?, ?, ?)"
  ).run(req.user.id, local_id || null, name, color || "#6c5ce7");

  res.status(201).json({
    collection: { id: result.lastInsertRowid, name, color: color || "#6c5ce7" },
  });
});

// Delete collection
router.delete("/collections/:id", authMiddleware, (req, res) => {
  db.prepare("DELETE FROM collection_items WHERE collection_id = ?").run(req.params.id);
  db.prepare("DELETE FROM collections WHERE id = ? AND user_id = ?").run(req.params.id, req.user.id);
  res.json({ message: "Koleksiyon silindi" });
});

// Add item to collection
router.post("/collections/:id/items", authMiddleware, (req, res) => {
  const { content, item_type } = req.body;

  const result = db.prepare(
    "INSERT INTO collection_items (collection_id, content, item_type) VALUES (?, ?, ?)"
  ).run(req.params.id, content, item_type || "text");

  res.status(201).json({ item: { id: result.lastInsertRowid, content, item_type: item_type || "text" } });
});

// Add item to collection by local_id
router.post("/collections/local/:localId/items", authMiddleware, (req, res) => {
  const { content, item_type } = req.body;
  const col = db.prepare(
    "SELECT id FROM collections WHERE local_id = ? AND user_id = ?"
  ).get(req.params.localId, req.user.id);

  if (!col) {
    return res.status(404).json({ error: "Koleksiyon bulunamadı" });
  }

  // Duplike kontrolü
  const existing = db.prepare(
    "SELECT id FROM collection_items WHERE collection_id = ? AND content = ?"
  ).get(col.id, content);

  if (existing) {
    return res.status(200).json({ item: { id: existing.id } });
  }

  const result = db.prepare(
    "INSERT INTO collection_items (collection_id, content, item_type) VALUES (?, ?, ?)"
  ).run(col.id, content, item_type || "text");

  res.status(201).json({ item: { id: result.lastInsertRowid } });
});

// Update sync item content (ana pano düzenleme)
router.put("/sync/item", authMiddleware, (req, res) => {
  const { old_content, new_content } = req.body;
  if (!old_content || !new_content) {
    return res.status(400).json({ error: "old_content ve new_content gerekli" });
  }

  db.prepare(
    "UPDATE sync_items SET content = ? WHERE user_id = ? AND content = ?"
  ).run(new_content, req.user.id, old_content);

  res.json({ message: "Güncellendi" });
});

// Update collection item content (koleksiyon düzenleme)
router.put("/collections/local/:localId/items", authMiddleware, (req, res) => {
  const { old_content, new_content } = req.body;
  const col = db.prepare(
    "SELECT id FROM collections WHERE local_id = ? AND user_id = ?"
  ).get(req.params.localId, req.user.id);

  if (col && old_content && new_content) {
    db.prepare(
      "UPDATE collection_items SET content = ? WHERE collection_id = ? AND content = ?"
    ).run(new_content, col.id, old_content);
  }
  res.json({ message: "Güncellendi" });
});

// Delete collection by local_id
router.delete("/collections/local/:localId", authMiddleware, (req, res) => {
  const col = db.prepare(
    "SELECT id FROM collections WHERE local_id = ? AND user_id = ?"
  ).get(req.params.localId, req.user.id);

  if (col) {
    db.prepare("DELETE FROM collection_items WHERE collection_id = ?").run(col.id);
    db.prepare("DELETE FROM collections WHERE id = ?").run(col.id);
  }
  res.json({ message: "Koleksiyon silindi" });
});

// Rename collection by local_id
router.put("/collections/local/:localId", authMiddleware, (req, res) => {
  const { name, color } = req.body;

  if (name) {
    db.prepare("UPDATE collections SET name = ? WHERE local_id = ? AND user_id = ?")
      .run(name, req.params.localId, req.user.id);
  }
  if (color) {
    db.prepare("UPDATE collections SET color = ? WHERE local_id = ? AND user_id = ?")
      .run(color, req.params.localId, req.user.id);
  }
  res.json({ message: "Koleksiyon güncellendi" });
});

// Remove item from collection by content match
router.delete("/collections/local/:localId/items", authMiddleware, (req, res) => {
  const { content } = req.body;
  const col = db.prepare(
    "SELECT id FROM collections WHERE local_id = ? AND user_id = ?"
  ).get(req.params.localId, req.user.id);

  if (col && content) {
    db.prepare("DELETE FROM collection_items WHERE collection_id = ? AND content = ?")
      .run(col.id, content);
  }
  res.json({ message: "Öğe koleksiyondan silindi" });
});

// Full sync endpoint — replace clipboard items with current 20
router.post("/sync/full", authMiddleware, (req, res) => {
  const { clipboard_items, collections } = req.body;
  const device_id = req.body.device_id || 0;

  db.transaction(() => {
    // Clipboard items: sil ve güncel 20'yi yaz
    if (clipboard_items && Array.isArray(clipboard_items)) {
      db.prepare("DELETE FROM sync_items WHERE user_id = ?").run(req.user.id);

      const insert = db.prepare(
        "INSERT INTO sync_items (user_id, device_id, content, item_type) VALUES (?, ?, ?, ?)"
      );

      // Sadece son 20'yi kaydet
      const limited = clipboard_items.slice(0, 20);
      for (const item of limited) {
        insert.run(req.user.id, device_id, item.content, item.item_type || "text");
      }
    }

    // Collections: replace all
    if (collections && Array.isArray(collections)) {
      const existingCols = db.prepare("SELECT id FROM collections WHERE user_id = ?").all(req.user.id);
      for (const col of existingCols) {
        db.prepare("DELETE FROM collection_items WHERE collection_id = ?").run(col.id);
      }
      db.prepare("DELETE FROM collections WHERE user_id = ?").run(req.user.id);

      const insertCol = db.prepare(
        "INSERT INTO collections (user_id, local_id, name, color) VALUES (?, ?, ?, ?)"
      );
      const insertItem = db.prepare(
        "INSERT INTO collection_items (collection_id, content, item_type) VALUES (?, ?, ?)"
      );

      for (const col of collections) {
        const result = insertCol.run(req.user.id, col.id || null, col.name, col.color || "#6c5ce7");
        if (col.items && Array.isArray(col.items)) {
          for (const item of col.items) {
            insertItem.run(result.lastInsertRowid, item.content, item.item_type || "text");
          }
        }
      }
    }
  })();

  res.json({ message: "Full sync tamamlandı" });
});

module.exports = router;
