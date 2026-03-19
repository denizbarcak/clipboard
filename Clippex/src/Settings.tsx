import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SettingsData {
  shortcut: string;
  autostart: boolean;
}

export default function Settings() {
  const [activeTab, setActiveTab] = useState<"general" | "account">("general");
  const [shortcut, setShortcut] = useState("Shift+Alt+C");
  const [autoStart, setAutoStart] = useState(false);
  const [recording, setRecording] = useState(false);
  const [saved, setSaved] = useState(false);
  const [hasChanges, setHasChanges] = useState(false);

  // Original values to detect changes
  const originalRef = useRef({ shortcut: "", autoStart: false });

  // Account state
  const [isLoggedIn, setIsLoggedIn] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [isRegister, setIsRegister] = useState(false);
  const [authError, setAuthError] = useState("");

  useEffect(() => {
    invoke<SettingsData>("get_settings").then((s) => {
      setShortcut(s.shortcut);
      setAutoStart(s.autostart);
      originalRef.current = { shortcut: s.shortcut, autoStart: s.autostart };
    });

    // Check existing session
    const token = localStorage.getItem("clippex_token");
    const user = localStorage.getItem("clippex_user");
    if (token && user) {
      const parsed = JSON.parse(user);
      setEmail(parsed.email);
      setIsLoggedIn(true);
    }
  }, []);

  useEffect(() => {
    const changed =
      shortcut !== originalRef.current.shortcut ||
      autoStart !== originalRef.current.autoStart;
    setHasChanges(changed);
    setSaved(false);
  }, [shortcut, autoStart]);

  const handleShortcutRecord = (e: React.KeyboardEvent) => {
    if (!recording) return;
    e.preventDefault();
    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");
    const key = e.key;
    if (!["Control", "Alt", "Shift", "Meta"].includes(key)) {
      parts.push(key.length === 1 ? key.toUpperCase() : key);
      setShortcut(parts.join("+"));
      setRecording(false);
    }
  };

  const [saveError, setSaveError] = useState("");

  const handleSave = async () => {
    try {
      setSaveError("");
      if (shortcut !== originalRef.current.shortcut) {
        // Tauri kısayol formatı: modifier'lar lowercase, tuş büyük harf
        const formatted = shortcut
          .split("+")
          .map((part, i, arr) =>
            i === arr.length - 1 ? part.toUpperCase() : part.toLowerCase()
          )
          .join("+");
        await invoke("update_shortcut", { shortcut: formatted });
      }
      if (autoStart !== originalRef.current.autoStart) {
        await invoke("toggle_autostart", { enabled: autoStart });
      }
      originalRef.current = { shortcut, autoStart };
      setHasChanges(false);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (err: any) {
      console.error("Kaydetme hatası:", err);
      setSaveError(typeof err === "string" ? err : err?.message || "Kaydetme başarısız");
    }
  };

  const API_URL = "http://localhost:3456/api";

  const handleAuth = async () => {
    setAuthError("");
    if (isRegister && password !== confirmPassword) {
      setAuthError("Şifreler eşleşmiyor");
      return;
    }
    if (!email || !password) {
      setAuthError("E-posta ve şifre gerekli");
      return;
    }
    if (isRegister && password.length < 6) {
      setAuthError("Şifre en az 6 karakter olmalı");
      return;
    }

    try {
      const endpoint = isRegister ? "/auth/register" : "/auth/login";
      const response = await fetch(`${API_URL}${endpoint}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password }),
      });

      const data = await response.json();

      if (!response.ok) {
        setAuthError(data.error || "Bir hata oluştu");
        return;
      }

      // Token'ı kaydet
      localStorage.setItem("clippex_token", data.token);
      localStorage.setItem("clippex_user", JSON.stringify(data.user));

      // Rust tarafına bildir (sync başlasın)
      await invoke("sync_login", { token: data.token, email: data.user.email });

      setIsLoggedIn(true);
      setPassword("");
      setConfirmPassword("");
    } catch (err) {
      setAuthError("Sunucuya bağlanılamadı");
    }
  };

  const handleLogout = async () => {
    localStorage.removeItem("clippex_token");
    localStorage.removeItem("clippex_user");
    await invoke("sync_logout");
    setIsLoggedIn(false);
    setEmail("");
  };

  return (
    <div className="settings-window">
      <div className="settings-sidebar">
        <div className="settings-logo">Clippex</div>
        <button
          className={`settings-tab ${activeTab === "general" ? "active" : ""}`}
          onClick={() => setActiveTab("general")}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3" />
            <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
          </svg>
          Genel
        </button>
        <button
          className={`settings-tab ${activeTab === "account" ? "active" : ""}`}
          onClick={() => setActiveTab("account")}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
            <circle cx="12" cy="7" r="4" />
          </svg>
          Hesap
        </button>
      </div>

      <div className="settings-content">
        {activeTab === "general" && (
          <div className="settings-panel">
            <h2>Genel Ayarlar</h2>

            <div className="setting-group">
              <label>Kısayol Tuşu</label>
              <div
                className={`shortcut-input ${recording ? "recording" : ""}`}
                tabIndex={0}
                onClick={() => setRecording(true)}
                onKeyDown={handleShortcutRecord}
                onBlur={() => setRecording(false)}
              >
                {recording ? "Tuş kombinasyonuna basın..." : shortcut}
              </div>
            </div>

            <div className="setting-group">
              <label>Başlangıçta Aç</label>
              <div className="toggle-row">
                <span className="toggle-desc">Bilgisayar açıldığında Clippex otomatik başlasın</span>
                <button
                  className={`toggle ${autoStart ? "on" : ""}`}
                  onClick={() => setAutoStart(!autoStart)}
                >
                  <div className="toggle-thumb" />
                </button>
              </div>
            </div>

            <button
              className={`save-btn ${saved ? "saved" : ""} ${!hasChanges && !saved ? "disabled" : ""}`}
              onClick={handleSave}
              disabled={!hasChanges && !saved}
            >
              {saved ? "Kaydedildi" : "Kaydet"}
            </button>
            {saveError && <div className="auth-error">{saveError}</div>}
          </div>
        )}

        {activeTab === "account" && (
          <div className="settings-panel">
            <h2>{isLoggedIn ? "Hesabım" : isRegister ? "Kayıt Ol" : "Giriş Yap"}</h2>

            {isLoggedIn ? (
              <div className="account-info">
                <p>Giriş yapıldı: {email}</p>
                <button className="logout-btn" onClick={handleLogout}>
                  Çıkış Yap
                </button>
              </div>
            ) : (
              <div className="auth-form">
                <div className="setting-group">
                  <label>E-posta</label>
                  <input
                    type="email"
                    className="settings-input"
                    placeholder="ornek@email.com"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                  />
                </div>

                <div className="setting-group">
                  <label>Şifre</label>
                  <input
                    type="password"
                    className="settings-input"
                    placeholder="••••••••"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                </div>

                {isRegister && (
                  <div className="setting-group">
                    <label>Şifre Tekrar</label>
                    <input
                      type="password"
                      className="settings-input"
                      placeholder="••••••••"
                      value={confirmPassword}
                      onChange={(e) => setConfirmPassword(e.target.value)}
                    />
                  </div>
                )}

                {authError && <div className="auth-error">{authError}</div>}

                <button className="save-btn" onClick={handleAuth}>
                  {isRegister ? "Kayıt Ol" : "Giriş Yap"}
                </button>

                <button
                  className="switch-auth"
                  onClick={() => {
                    setIsRegister(!isRegister);
                    setAuthError("");
                  }}
                >
                  {isRegister
                    ? "Zaten hesabın var mı? Giriş yap"
                    : "Hesabın yok mu? Kayıt ol"}
                </button>
              </div>
            )}

            <div className="sync-info">
              <h3>Cihazlar Arası Senkronizasyon</h3>
              <p>
                Hesap oluşturarak kopyaladığın metinleri, görselleri ve
                koleksiyonları tüm cihazlarında senkronize edebilirsin.
              </p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
