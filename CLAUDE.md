# Vela — Terminal SFTP/SCP Client

## Ziel
WinSCP-Klon als TUI für macOS. Zwei-Panel-Layout,
linkes Panel lokal, rechtes Panel remote.
Zielgruppe: Webentwickler die im Terminal arbeiten.

## Tech Stack
- Rust (stable)
- ratatui — TUI Framework
- ssh2 — SFTP und SCP
- serde + toml — Konfiguration/Profile speichern
- similar — Diff-Ansicht
- crossterm — Terminal-Backend für ratatui
- thiserror — Fehlerbehandlung

## Projektstruktur
src/
├── main.rs
├── app.rs          ← App-State, Event-Loop
├── ui/
│   ├── mod.rs
│   ├── panels.rs   ← Dual-Panel Layout
│   ├── statusbar.rs
│   └── dialogs.rs  ← Verbindung, Rename, Delete etc.
├── connection/
│   ├── mod.rs
│   ├── sftp.rs
│   └── scp.rs
├── config/
│   ├── mod.rs
│   └── profiles.rs ← Gespeicherte Verbindungsprofile
└── transfer/
    ├── mod.rs
    └── queue.rs    ← Transfer-Queue mit Fortschritt

## Konfiguration
Profile gespeichert in ~/.config/vela/profiles.toml
Format:
  [[profile]]
  name = "Mein Server"
  host = "example.com"
  port = 22
  user = "deploy"
  auth = "key"          # oder "password"
  key_path = "~/.ssh/id_rsa"

## Core Features (Phase 1)
- Dual-Panel: lokal links, remote rechts
- Navigation mit Pfeiltasten, Tab wechselt Panel
- Verbindungsprofile laden/speichern/löschen
- Dateiliste mit Name, Größe, Datum, Permissions
- Upload (F5) und Download (F6)
- Löschen (F8) mit Bestätigungsdialog
- Verzeichnis erstellen (F7)
- Rename (F2)

## Phase 2
- Dateien direkt remote bearbeiten
  (temporäre lokale Kopie → $EDITOR → Upload bei Änderung)
- Diff lokal vs remote (similar crate)
- Permissions ändern (chmod) via Dialog
- SCP als Alternative zu SFTP
- Transfer-Queue mit Fortschrittsanzeige

## Tastaturlayout (WinSCP-Stil)
Tab     → Panel wechseln
Enter   → Verzeichnis öffnen / Datei bearbeiten
F2      → Rename
F4      → Bearbeiten (remote → lokal → Editor → Upload)
F5      → Kopieren/Download/Upload
F6      → Verschieben
F7      → Verzeichnis erstellen
F8      → Löschen
F10     → Beenden
Esc     → Dialog schließen / abbrechen
/       → Suchen/Filtern

## Coding-Regeln
- Kein unwrap() in Production-Code, immer ? oder match
- Fehler mit thiserror definieren
- Jede Funktion max 40 Zeilen
- Async nur wo nötig (ssh2 ist synchron)
- Kommentare auf Englisch

## Was Claude NICHT tun soll
- Kein Electron, keine GUI-Frameworks
- Keine Cloud-Services oder Telemetrie
- Nicht alles auf einmal bauen — Phase 1 zuerst fertigstellen
