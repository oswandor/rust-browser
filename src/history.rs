//! Historial global de navegación persistido en JSON.
//!
//! Se guarda en `%APPDATA%\rust-browser\history.json` (Windows) con
//! write-through en cada visita y tope de 5000 entradas.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    /// Segundos desde el epoch Unix.
    pub visited_at: u64,
}

#[derive(Debug)]
pub struct History {
    pub entries: Vec<HistoryEntry>,
    path: PathBuf,
}

impl History {
    /// Carga el historial desde disco (archivo inexistente = historial vacío).
    pub fn load() -> Self {
        let path = history_path();
        let entries = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { entries, path }
    }

    /// Registra una visita y persiste inmediatamente.
    /// Ignora páginas internas (`about:blank`) y duplicados consecutivos.
    pub fn record(&mut self, url: &str, title: &str) {
        if url == "about:blank" || url.is_empty() {
            return;
        }
        if let Some(last) = self.entries.last() {
            if last.url == url {
                return;
            }
        }
        self.entries.push(HistoryEntry {
            url: url.to_string(),
            title: title.to_string(),
            visited_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
        if self.entries.len() > MAX_ENTRIES {
            let overflow = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..overflow);
        }
        self.save();
    }

    /// Escribe el historial a disco. Los fallos se ignoran a conciencia:
    /// perder el historial no debe tirar el navegador.
    pub fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.entries) {
            let _ = fs::write(&self.path, json);
        }
    }
}

fn history_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rust-browser")
        .join("history.json")
}
