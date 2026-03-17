import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  ClipboardIcon,
  TextIcon,
  LinkIcon,
  FolderIcon,
  ImageIcon,
  TrashIcon,
  PlusIcon,
  ChevronLeftIcon,
  SearchIcon,
  MoreIcon,
} from "./icons";

interface ClipboardItem {
  id: string;
  content: string;
  item_type: string;
  source_app: string | null;
  preview: string | null;
  is_pinned: boolean;
  created_at: string;
  collection_color: string | null;
}

interface Collection {
  id: string;
  name: string;
  color: string;
  created_at: string;
}

interface ContextMenu {
  x: number;
  y: number;
  collection: Collection;
}

interface CardContextMenu {
  item: ClipboardItem;
  showCollections: boolean;
  cardRect: { x: number; y: number; width: number; height: number };
}

interface EditingItem {
  item: ClipboardItem;
  value: string;
}

interface DeleteConfirm {
  collection: Collection;
  itemCount: number;
}

const COLLECTION_COLORS = [
  "#6c5ce7", // Mor
  "#e17055", // Turuncu
  "#00b894", // Yeşil
  "#0984e3", // Mavi
  "#e84393", // Pembe
  "#fdcb6e", // Sarı
  "#00cec9", // Turkuaz
  "#d63031", // Kırmızı
  "#636e72", // Gri
];

function App() {
  const [items, setItems] = useState<ClipboardItem[]>([]);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [activeCollection, setActiveCollectionRaw] = useState<string | null>(
    () => localStorage.getItem("activeCollection")
  );
  const setActiveCollection = (id: string | null) => {
    setActiveCollectionRaw(id);
    if (id) {
      localStorage.setItem("activeCollection", id);
    } else {
      localStorage.removeItem("activeCollection");
    }
  };
  const [dragOverCollection, setDragOverCollection] = useState<string | null>(null);
  const [draggingItem, setDraggingItem] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [contextMenu, setContextMenu] = useState<ContextMenu | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [cardContextMenu, setCardContextMenu] = useState<CardContextMenu | null>(null);
  const [editing, setEditing] = useState<EditingItem | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<DeleteConfirm | null>(null);
  const [isSearching, setIsSearching] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchColorFilter, setSearchColorFilter] = useState<string | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [currentShortcut, setCurrentShortcut] = useState("ctrl+alt+v");
  const cardsRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);
  const ghostRef = useRef<HTMLDivElement | null>(null);
  const dragItemRef = useRef<ClipboardItem | null>(null);
  const dragOverRef = useRef<string | null>(null);
  const dragCleanupRef = useRef<(() => void) | null>(null);

  const openSettingsWindow = async () => {
    const existing = await WebviewWindow.getByLabel("settings");
    if (existing) {
      await existing.show();
      await existing.setFocus();
      return;
    }
    new WebviewWindow("settings", {
      url: "index.html",
      title: "Clippex Ayarlar",
      width: 500,
      height: 600,
      resizable: false,
      center: true,
    });
  };

  const loadItems = useCallback(async () => {
    try {
      if (isSearching) {
        if (searchQuery.trim()) {
          const results = await invoke<ClipboardItem[]>("search_all", {
            query: searchQuery,
            color: searchColorFilter,
          });
          setItems(results);
        } else if (searchColorFilter) {
          // Renk filtresi seçili ama arama boş — o renkteki tüm koleksiyon öğelerini göster
          const results = await invoke<ClipboardItem[]>("search_all", {
            query: "",
            color: searchColorFilter,
          });
          setItems(results);
        } else {
          // Arama modu açık ama hiçbir şey yazılmamış — ana pano göster
          const results = await invoke<ClipboardItem[]>("get_clipboard_items", {
            limit: 20,
            offset: 0,
          });
          setItems(results);
        }
      } else if (activeCollection) {
        const results = await invoke<ClipboardItem[]>("get_collection_items", {
          collectionId: activeCollection,
        });
        setItems(results);
      } else {
        const results = await invoke<ClipboardItem[]>("get_clipboard_items", {
          limit: 20,
          offset: 0,
        });
        setItems(results);
      }
    } catch (err) {
      console.error("Öğeler yüklenemedi:", err);
    }
  }, [activeCollection, isSearching, searchQuery, searchColorFilter]);

  const loadCollections = useCallback(async () => {
    try {
      const results = await invoke<Collection[]>("get_collections");
      setCollections(results);
    } catch (err) {
      console.error("Koleksiyonlar yüklenemedi:", err);
    }
  }, []);

  useEffect(() => {
    loadItems();
    loadCollections();
    const unlisten = listen<ClipboardItem>("clipboard-changed", () => {
      if (!activeCollection) loadItems();
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [loadItems, loadCollections, activeCollection]);

  // Ayarları yükle
  useEffect(() => {
    invoke<{ shortcut: string }>("get_settings").then((s) => {
      setCurrentShortcut(s.shortcut);
    }).catch(console.error);
  }, []);

  // Tüm native context menu'yu engelle
  useEffect(() => {
    const prevent = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", prevent);
    return () => document.removeEventListener("contextmenu", prevent);
  }, []);

  // Pencere gizlendiğinde state temizle
  useEffect(() => {
    const unlistenBlur = listen("tauri://blur", () => {
      setCardContextMenu(null);
      setContextMenu(null);
      setEditing(null);
      setIsSearching(false);
      setSearchQuery("");
      setSearchColorFilter(null);
    });
    const unlistenShown = listen("window-shown", () => {
      setIsSearching(false);
      setSearchQuery("");
      setSearchColorFilter(null);
      setCardContextMenu(null);
      setContextMenu(null);
      setEditing(null);
      // Sürükleme listener'larını temizle
      if (dragCleanupRef.current) {
        dragCleanupRef.current();
      } else {
        setDraggingItem(null);
        setDragOverCollection(null);
        isDraggingRef.current = false;
        dragItemRef.current = null;
        dragOverRef.current = null;
        removeGhost();
        invoke("set_dragging", { dragging: false }).catch(() => {});
      }
    });
    return () => {
      unlistenBlur.then((fn) => fn());
      unlistenShown.then((fn) => fn());
    };
  }, []);

  // ESC tuşu + context menu kapat
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (editing) {
          setEditing(null);
        } else if (cardContextMenu) {
          setCardContextMenu(null);
        } else if (contextMenu) {
          setContextMenu(null);
        } else if (renamingId) {
          setRenamingId(null);
        } else if (isSearching) {
          setIsSearching(false);
          setSearchQuery("");
          setSearchColorFilter(null);
        } else if (activeCollection) {
          setActiveCollection(null);
        } else {
          invoke("hide_window").catch(console.error);
        }
      }
    };
    const handleClick = () => {
      if (contextMenu) setContextMenu(null);
      if (cardContextMenu) setCardContextMenu(null);
    };
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("click", handleClick);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("click", handleClick);
    };
  }, [activeCollection, contextMenu, cardContextMenu, renamingId, editing, isSearching]);

  const handlePaste = async (item: ClipboardItem) => {
    try {
      await invoke("paste_to_previous_window", { content: item.content });
    } catch (err) {
      console.error("Yapıştırılamadı:", err);
    }
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    try {
      if (activeCollection) {
        await invoke("remove_from_collection", { itemId: id, collectionId: activeCollection.id });
      } else {
        await invoke("delete_clipboard_item", { id });
      }
      loadItems();
    } catch (err) {
      console.error("Silme hatası:", err);
    }
  };

  // === Koleksiyon işlemleri ===
  const handleCreateCollection = async () => {
    if (!newName.trim()) return;
    try {
      await invoke("create_collection", { name: newName.trim() });
      setNewName("");
      setIsCreating(false);
      loadCollections();
    } catch (err) {
      console.error("Koleksiyon oluşturulamadı:", err);
    }
  };

  const handleDeleteCollectionConfirm = async (col: Collection) => {
    try {
      const count = await invoke<number>("get_collection_item_count", { collectionId: col.id });
      if (count > 0) {
        setDeleteConfirm({ collection: col, itemCount: count });
      } else {
        await invoke("delete_collection", { id: col.id });
        if (activeCollection === col.id) setActiveCollection(null);
        loadCollections();
        loadItems();
      }
    } catch (err) {
      console.error("Koleksiyon silinemedi:", err);
    }
  };

  const handleDeleteCollectionFinal = async () => {
    if (!deleteConfirm) return;
    try {
      await invoke("delete_collection", { id: deleteConfirm.collection.id });
      if (activeCollection === deleteConfirm.collection.id) setActiveCollection(null);
      setDeleteConfirm(null);
      loadCollections();
      loadItems();
    } catch (err) {
      console.error("Koleksiyon silinemedi:", err);
    }
  };

  const handleRenameCollection = async (id: string) => {
    if (!renameValue.trim()) { setRenamingId(null); return; }
    try {
      await invoke("rename_collection", { id, name: renameValue.trim() });
      setRenamingId(null);
      loadCollections();
    } catch (err) {
      console.error("Rename hatası:", err);
    }
  };

  const handleChangeColor = async (id: string, color: string) => {
    try {
      await invoke("update_collection_color", { id, color });
      setContextMenu(null);
      loadCollections();
    } catch (err) {
      console.error("Renk değiştirme hatası:", err);
    }
  };

  // Kart düzenleme kaydet
  const handleSaveEdit = async () => {
    if (!editing) return;
    try {
      await invoke("update_clipboard_content", {
        id: editing.item.id,
        content: editing.value,
        oldContent: editing.item.content,
        isCollectionItem: !!activeCollection,
        collectionId: activeCollection?.id || null,
      });
      setEditing(null);
      loadItems();
    } catch (err) {
      console.error("Düzenleme hatası:", err);
    }
  };

  // Kart sağ tık — direkt yeni kartın menüsünü aç
  const handleCardContextMenu = (e: React.MouseEvent, item: ClipboardItem) => {
    e.preventDefault();
    e.stopPropagation();
    const card = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setCardContextMenu({
      item,
      showCollections: false,
      cardRect: { x: card.left, y: card.top, width: card.width, height: card.height },
    });
  };

  // Koleksiyon sağ tık
  const handleContextMenu = (e: React.MouseEvent, col: Collection) => {
    e.preventDefault();
    e.stopPropagation();
    setContextMenu({ x: e.clientX, y: e.clientY, collection: col });
  };

  // === Drag & Drop ===
  const createGhost = (item: ClipboardItem, x: number, y: number) => {
    const ghost = document.createElement("div");
    ghost.className = "drag-ghost";
    ghost.textContent = item.content.length > 40
      ? item.content.slice(0, 40) + "..."
      : item.content;
    ghost.style.left = x + "px";
    ghost.style.top = y + "px";
    document.body.appendChild(ghost);
    ghostRef.current = ghost;
  };

  const moveGhost = (x: number, y: number) => {
    if (ghostRef.current) {
      ghostRef.current.style.left = x + "px";
      ghostRef.current.style.top = y + "px";
    }
  };

  const removeGhost = () => {
    if (ghostRef.current) { ghostRef.current.remove(); ghostRef.current = null; }
  };

  const handleCardMouseDown = (e: React.MouseEvent, item: ClipboardItem) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const startX = e.clientX;
    const startY = e.clientY;

    const onMouseMove = (moveEvent: MouseEvent) => {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;
      if (!isDraggingRef.current && (Math.abs(dx) + Math.abs(dy) > 10)) {
        isDraggingRef.current = true;
        dragItemRef.current = item;
        setDraggingItem(item.id);
        invoke("set_dragging", { dragging: true }).catch(() => {});
        createGhost(item, moveEvent.clientX, moveEvent.clientY);
      }
      if (isDraggingRef.current) {
        moveGhost(moveEvent.clientX, moveEvent.clientY);
        const el = document.elementFromPoint(moveEvent.clientX, moveEvent.clientY);
        const pill = el?.closest("[data-collection-id]") as HTMLElement | null;
        const colId = pill?.getAttribute("data-collection-id") || null;
        dragOverRef.current = colId;
        setDragOverCollection(colId);
      }
    };

    const cleanup = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      removeGhost();
      invoke("set_dragging", { dragging: false }).catch(() => {});
      isDraggingRef.current = false;
      dragItemRef.current = null;
      dragOverRef.current = null;
      dragCleanupRef.current = null;
      setDraggingItem(null);
      setDragOverCollection(null);
    };

    const onMouseUp = async () => {
      if (isDraggingRef.current && dragOverRef.current && dragItemRef.current) {
        try {
          await invoke("add_item_to_collection", {
            collectionId: dragOverRef.current,
            itemId: dragItemRef.current.id,
            content: dragItemRef.current.content,
            itemType: dragItemRef.current.item_type,
            preview: dragItemRef.current.preview,
          });
          loadItems();
        } catch (err) {
          console.error("Koleksiyona eklenemedi:", err);
        }
      } else if (!isDraggingRef.current) {
        handlePaste(item);
      }
      cleanup();
    };

    dragCleanupRef.current = cleanup;
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  const formatTime = (dateStr: string) => {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return "Az önce";
    if (diffMin < 60) return `${diffMin}dk`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour}sa`;
    const diffDay = Math.floor(diffHour / 24);
    return `${diffDay}g`;
  };

  const TypeIcon = ({ type }: { type: string }) => {
    switch (type) {
      case "Link": return <LinkIcon />;
      case "FilePath": return <FolderIcon />;
      case "Image": return <ImageIcon />;
      default: return <TextIcon />;
    }
  };

  const typeLabel = (type: string) => {
    switch (type) {
      case "Link": return "Link";
      case "FilePath": return "Dosya";
      case "Image": return "Resim";
      default: return "Metin";
    }
  };

  return (
    <div className="app-container" onClick={() => {
      setCardContextMenu(null);
      setContextMenu(null);
      if (isSearching) { setIsSearching(false); setSearchQuery(""); setSearchColorFilter(null); }
    }}>
      {/* Collection Bar */}
      <div className="collection-bar">
        {activeCollection && !isSearching && (
          <button
            className="collection-back-btn"
            onClick={() => setActiveCollection(null)}
            title="Panoya dön"
          >
            <ChevronLeftIcon />
          </button>
        )}

        {/* Arama ikonu / açık input */}
        {isSearching ? (
          <>
            <div className="search-expand" onClick={(e) => e.stopPropagation()}>
              <span className="search-expand-icon"><SearchIcon /></span>
              <input
                ref={searchInputRef}
                className="search-expand-input"
                type="text"
                placeholder="Ara..."
                value={searchQuery}
                onChange={(e) => {
                  const q = e.target.value;
                  setSearchQuery(q);
                  // Direkt arama yap
                  if (q.trim()) {
                    const args: Record<string, unknown> = { query: q };
                    if (searchColorFilter) args.color = searchColorFilter;
                    invoke<ClipboardItem[]>("search_all", args)
                      .then((results) => { setItems(results); })
                      .catch((err) => { console.error("Arama hatası:", err); });
                  } else if (searchColorFilter) {
                    invoke<ClipboardItem[]>("search_all", { query: "", color: searchColorFilter })
                      .then((results) => { setItems(results); })
                      .catch(console.error);
                  } else {
                    invoke<ClipboardItem[]>("get_clipboard_items", { limit: 20, offset: 0 })
                      .then((results) => { setItems(results); })
                      .catch(console.error);
                  }
                }}
                autoFocus
              />
              <button
                className="search-close"
                onClick={() => { setIsSearching(false); setSearchQuery(""); setSearchColorFilter(null); }}
              >
                ×
              </button>
            </div>
            {collections.length > 0 && (
              <div className="search-color-filters" onClick={(e) => e.stopPropagation()}>
                {[...new Set(collections.map(c => c.color))].map((color) => (
                  <button
                    key={color}
                    className={`search-color-dot ${searchColorFilter === color ? "active" : ""}`}
                    style={{ background: color }}
                    onClick={() => setSearchColorFilter(searchColorFilter === color ? null : color)}
                    title={collections.filter(c => c.color === color).map(c => c.name).join(", ")}
                  />
                ))}
              </div>
            )}
          </>
        ) : (
          <>
            <button
              className="search-trigger"
              onClick={() => { setIsSearching(true); setActiveCollection(null); }}
              title="Ara"
            >
              <SearchIcon />
            </button>

            {collections.map((col) => (
              <div
                key={col.id}
                data-collection-id={col.id}
                className={`collection-pill ${activeCollection === col.id ? "active" : ""} ${dragOverCollection === col.id ? "drag-over" : ""}`}
                onClick={() => { if (!renamingId) setActiveCollection(activeCollection === col.id ? null : col.id); }}
                onContextMenu={(e) => handleContextMenu(e, col)}
                style={{ "--pill-color": col.color } as React.CSSProperties}
              >
                <span className="pill-dot" />
                {renamingId === col.id ? (
                  <input
                    className="pill-rename-input"
                    value={renameValue}
                    onChange={(e) => setRenameValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleRenameCollection(col.id);
                      if (e.key === "Escape") setRenamingId(null);
                    }}
                    onBlur={() => handleRenameCollection(col.id)}
                    onClick={(e) => e.stopPropagation()}
                    autoFocus
                  />
                ) : (
                  <span className="pill-name">{col.name}</span>
                )}
              </div>
            ))}

            {isCreating ? (
              <div className="collection-create-input">
                <input
                  type="text"
                  placeholder="Ad..."
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleCreateCollection();
                    if (e.key === "Escape") { setIsCreating(false); setNewName(""); }
                  }}
                  onBlur={() => { if (newName.trim()) { handleCreateCollection(); } else { setIsCreating(false); } }}
                  autoFocus
                />
              </div>
            ) : (
              <button
                className="collection-add-btn"
                onClick={() => setIsCreating(true)}
                title="Yeni koleksiyon"
              >
                <PlusIcon />
              </button>
            )}
          </>
        )}

        {/* Sağ köşe: 3 nokta ayar */}
        <button
          className="settings-trigger"
          onClick={(e) => { e.stopPropagation(); openSettingsWindow(); }}
          title="Ayarlar"
        >
          <MoreIcon />
        </button>
      </div>

      {/* Cards */}
      {items.length === 0 ? (
        <div className="empty-state">
          <div className="empty-icon"><ClipboardIcon /></div>
          <div className="empty-text">
            {activeCollection ? "Bu koleksiyon boş" : "Clipboard geçmişi boş"}
          </div>
          <div className="empty-hint">
            {activeCollection ? "Kartları sürükleyip buraya bırak" : "Bir şey kopyaladığında burada görünecek"}
          </div>
        </div>
      ) : (
        <div
          className="cards-area"
          ref={cardsRef}
          onWheel={(e) => {
            if (cardsRef.current) {
              e.preventDefault();
              cardsRef.current.scrollLeft += e.deltaY * 5;
            }
          }}
        >
          {items.map((item) => {
            const cardColor = activeCollection
              ? collections.find(c => c.id === activeCollection)?.color
              : item.collection_color;
            return (
            <div
              key={item.id}
              className={`clip-card ${draggingItem === item.id ? "dragging" : ""} ${cardColor ? "in-collection" : ""}`}
              style={cardColor ? { "--collection-color": cardColor } as React.CSSProperties : undefined}
              onMouseDown={!activeCollection ? (e) => handleCardMouseDown(e, item) : undefined}
              onClick={activeCollection ? () => handlePaste(item) : undefined}
              onContextMenu={(e) => handleCardContextMenu(e, item)}
            >
              <div className="card-content">
                {item.item_type === "Link" ? (
                  <span className="card-link">{item.content}</span>
                ) : item.item_type === "FilePath" ? (
                  <span className="card-filepath">{item.content}</span>
                ) : (
                  <span className="card-text">{item.content}</span>
                )}
              </div>
              <div className="card-footer">
                <div className="card-meta">
                  <span className="card-type-badge">
                    <TypeIcon type={item.item_type} />
                    {typeLabel(item.item_type)}
                  </span>
                  <span className="card-time">{item.content.length} karakter</span>
                </div>
                {activeCollection && (
                  <div className="card-actions" style={{ opacity: 1 }}>
                    <button
                      className="card-action-btn delete"
                      onClick={(e) => handleDelete(e, item.id)}
                      title="Koleksiyondan çıkar"
                    >
                      <TrashIcon />
                    </button>
                  </div>
                )}
              </div>
            </div>
          );
          })}
        </div>
      )}

      {/* Settings artık ayrı pencerede açılıyor */}

      {/* Card Context Menu */}
      {cardContextMenu && (
        <div className="card-context-overlay">
          <div
            className="card-context-menu"
            style={{
              left: cardContextMenu.cardRect.x + cardContextMenu.cardRect.width - 10,
              top: cardContextMenu.cardRect.y - 5,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              className="ctx-item"
              onClick={() => {
                setEditing({ item: cardContextMenu.item, value: cardContextMenu.item.content });
                setCardContextMenu(null);
              }}
            >
              Düzenle
            </button>
            {!activeCollection && collections.length > 0 && (
              <div className="ctx-submenu-wrap">
                <button className="ctx-item">
                  Koleksiyona ekle
                  <span className="ctx-arrow">›</span>
                </button>
                <div className="ctx-submenu">
                  {collections.map((col) => (
                    <button
                      key={col.id}
                      className="ctx-item ctx-collection"
                      onClick={async () => {
                        try {
                          await invoke("add_item_to_collection", {
                            collectionId: col.id,
                            itemId: cardContextMenu.item.id,
                            content: cardContextMenu.item.content,
                            itemType: cardContextMenu.item.item_type,
                            preview: cardContextMenu.item.preview,
                          });
                          loadItems();
                        } catch (err) {
                          console.error("Koleksiyona eklenemedi:", err);
                        }
                        setCardContextMenu(null);
                      }}
                    >
                      <span className="context-collection-dot" style={{ background: col.color }} />
                      {col.name}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Delete Confirm */}
      {deleteConfirm && (
        <div className="edit-overlay" onClick={() => setDeleteConfirm(null)}>
          <div className="delete-confirm" onClick={(e) => e.stopPropagation()}>
            <div className="delete-confirm-icon">
              <TrashIcon />
            </div>
            <div className="delete-confirm-title">
              "{deleteConfirm.collection.name}" silinsin mi?
            </div>
            <div className="delete-confirm-desc">
              Bu koleksiyondaki {deleteConfirm.itemCount} kayıt da birlikte silinecek.
            </div>
            <div className="delete-confirm-actions">
              <button className="edit-btn cancel" onClick={() => setDeleteConfirm(null)}>
                Vazgeç
              </button>
              <button className="delete-confirm-btn" onClick={handleDeleteCollectionFinal}>
                Sil
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Edit Modal */}
      {editing && (
        <div className="edit-overlay" onClick={() => setEditing(null)}>
          <div className="edit-modal" onClick={(e) => e.stopPropagation()}>
            <div className="edit-header">
              <button className="edit-btn cancel" onClick={() => setEditing(null)}>
                Vazgeç
              </button>
              <span className="edit-title">Düzenle</span>
              <button className="edit-btn save" onClick={handleSaveEdit}>
                Kaydet
              </button>
            </div>
            <textarea
              className="edit-textarea"
              value={editing.value}
              onChange={(e) => setEditing({ ...editing, value: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "s" && (e.ctrlKey || e.metaKey)) {
                  e.preventDefault();
                  handleSaveEdit();
                }
              }}
              autoFocus
            />
          </div>
        </div>
      )}

      {/* Collection Context Menu */}
      {contextMenu && (
        <div
          className="context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            className="context-menu-item"
            onClick={() => {
              setRenamingId(contextMenu.collection.id);
              setRenameValue(contextMenu.collection.name);
              setContextMenu(null);
            }}
          >
            Yeniden Adlandır
          </button>
          <button
            className="context-menu-item danger"
            onClick={() => {
              handleDeleteCollectionConfirm(contextMenu.collection);
              setContextMenu(null);
            }}
          >
            Sil
          </button>
          <div className="context-menu-divider" />
          <div className="context-menu-colors">
            {COLLECTION_COLORS.map((color) => (
              <button
                key={color}
                className={`color-dot ${contextMenu.collection.color === color ? "active" : ""}`}
                style={{ background: color }}
                onClick={() => handleChangeColor(contextMenu.collection.id, color)}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
