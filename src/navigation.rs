//! Pestañas y normalización de direcciones.
//!
//! Cada pestaña mantiene su propio historial de navegación (pila de URLs con
//! cursor) en Rust; el `WebView` físico es único y compartido (ver README).

use url::Url;

pub const HOME_URL: &str = "https://duckduckgo.com";
pub const NEW_TAB_URL: &str = "about:blank";

/// Una pestaña del navegador: título + historial de URLs con cursor.
#[derive(Debug)]
pub struct Tab {
    pub title: String,
    history: Vec<String>,
    cursor: usize,
    pub loading: bool,
}

impl Tab {
    pub fn new(url: &str) -> Self {
        Self {
            title: String::new(),
            history: vec![url.to_string()],
            cursor: 0,
            loading: false,
        }
    }

    /// URL actual de la pestaña.
    pub fn url(&self) -> &str {
        &self.history[self.cursor]
    }

    /// Registra una navegación nueva: descarta el "adelante" y apila la URL.
    pub fn push(&mut self, url: String) {
        if url == self.url() {
            return;
        }
        self.history.truncate(self.cursor + 1);
        self.history.push(url);
        self.cursor = self.history.len() - 1;
    }

    /// Sustituye la URL actual (p. ej. tras una redirección del servidor).
    pub fn replace_current(&mut self, url: &str) {
        if url != self.url() {
            self.history[self.cursor] = url.to_string();
        }
    }

    pub fn can_back(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_forward(&self) -> bool {
        self.cursor + 1 < self.history.len()
    }

    /// Retrocede en el historial y devuelve la URL a cargar, si se pudo.
    pub fn back(&mut self) -> Option<String> {
        if self.can_back() {
            self.cursor -= 1;
            Some(self.url().to_string())
        } else {
            None
        }
    }

    /// Avanza en el historial y devuelve la URL a cargar, si se pudo.
    pub fn forward(&mut self) -> Option<String> {
        if self.can_forward() {
            self.cursor += 1;
            Some(self.url().to_string())
        } else {
            None
        }
    }
}

/// Convierte lo que el usuario escribió en algo navegable.
///
/// - "example.com"        -> https://example.com
/// - "http://..." / "about:blank" -> tal cual
/// - "rust web framework" -> búsqueda en DuckDuckGo
pub fn normalize_input(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    // Ya trae esquema válido: usar tal cual.
    // Ojo: Url::parse("localhost:3000") da esquema "localhost"; se maneja abajo.
    if let Ok(url) = Url::parse(s) {
        if matches!(url.scheme(), "http" | "https" | "file" | "about" | "data") {
            return Some(s.to_string());
        }
    }

    // "localhost" y "localhost:8080" sin esquema.
    if s.split(':').next() == Some("localhost") {
        return Some(format!("http://{s}"));
    }

    // Parece un dominio: sin espacios, con punto y caracteres de URL.
    let looks_like_host = !s.contains(char::is_whitespace)
        && s.contains('.')
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,;=%".contains(c)
        });
    if looks_like_host {
        if let Ok(url) = Url::parse(&format!("https://{s}")) {
            if url.host_str().is_some() {
                return Some(format!("https://{s}"));
            }
        }
    }

    // Cualquier otra cosa: búsqueda web.
    Some(format!(
        "https://duckduckgo.com/?q={}",
        urlencode(s)
    ))
}

/// Codificación de query-string mínima (formato application/x-www-form-urlencoded).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dominio_simple_a_https() {
        assert_eq!(
            normalize_input("example.com"),
            Some("https://example.com".into())
        );
    }

    #[test]
    fn localhost_con_puerto() {
        assert_eq!(
            normalize_input("localhost:3000"),
            Some("http://localhost:3000".into())
        );
    }

    #[test]
    fn texto_libre_a_busqueda() {
        assert_eq!(
            normalize_input("rust web framework"),
            Some("https://duckduckgo.com/?q=rust+web+framework".into())
        );
    }

    #[test]
    fn url_completa_se_respeta() {
        assert_eq!(
            normalize_input("  http://a.b/c?d=1 "),
            Some("http://a.b/c?d=1".into())
        );
    }

    #[test]
    fn historial_de_pestaña() {
        let mut t = Tab::new("about:blank");
        t.push("https://a".into());
        t.push("https://b".into());
        assert_eq!(t.url(), "https://b");
        assert!(t.can_back() && !t.can_forward());
        assert_eq!(t.back(), Some("https://a".into()));
        // push desde el pasado descarta el futuro
        t.push("https://c".into());
        assert_eq!(t.url(), "https://c");
        assert!(!t.can_forward());
    }
}
