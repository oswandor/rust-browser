//! Creación y gestión del `WebView` (wry / WebView2 en Windows).
//!
//! El webview se crea como ventana hija de la winit y se posiciona bajo la
//! barra de pestañas+toolbar que pinta egui; sus callbacks envían eventos al
//! hilo principal vía `EventLoopProxy` de winit.

use winit::event_loop::EventLoopProxy;
use wry::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use wry::{Rect, WebView, WebViewBuilder};

use crate::app::UserEvent;

/// Script inyectado en cada página: reporta atajos de teclado por IPC.
///
/// Es necesario porque el foco del teclado vive en el HWND hijo del WebView2,
/// así que winit nunca ve Ctrl+T y compañía mientras se navega.
pub const SHORTCUTS_JS: &str = r#"
(function () {
  if (window.__rustBrowserShortcuts) return;
  window.__rustBrowserShortcuts = true;
  document.addEventListener('keydown', function (e) {
    var ctrl = e.ctrlKey || e.metaKey;
    var key = (e.key || '').toLowerCase();
    var msg = null;
    if (ctrl && key === 't') msg = 'ctrl-t';
    else if (ctrl && key === 'w') msg = 'ctrl-w';
    else if (ctrl && key === 'l') msg = 'ctrl-l';
    else if (key === 'f5' || (ctrl && key === 'r')) msg = 'reload';
    else if (e.altKey && key === 'arrowleft') msg = 'back';
    else if (e.altKey && key === 'arrowright') msg = 'forward';
    if (msg) {
      e.preventDefault();
      e.stopPropagation();
      if (window.ipc && window.ipc.postMessage) window.ipc.postMessage(msg);
    }
  }, true);
})();
"#;

/// Crea el webview hijo con todos los handlers cableados al event loop.
pub fn create(
    window: &winit::window::Window,
    proxy: &EventLoopProxy<UserEvent>,
    start_url: &str,
    bounds: Rect,
) -> Result<WebView, wry::Error> {
    let load_proxy = proxy.clone();
    let title_proxy = proxy.clone();
    let ipc_proxy = proxy.clone();

    WebViewBuilder::new()
        .with_url(start_url)
        .with_bounds(bounds)
        .with_initialization_script(SHORTCUTS_JS)
        .with_on_page_load_handler(move |event, url| {
            // Solo notificamos al hilo principal; el estado vive allí.
            let msg = match event {
                wry::PageLoadEvent::Started => UserEvent::PageLoadStarted(url),
                wry::PageLoadEvent::Finished => UserEvent::PageLoadFinished(url),
            };
            let _ = load_proxy.send_event(msg);
        })
        .with_document_title_changed_handler(move |title| {
            let _ = title_proxy.send_event(UserEvent::Title(title));
        })
        .with_ipc_handler(move |request| {
            // El cuerpo del mensaje IPC es el string enviado desde JS.
            let _ = ipc_proxy.send_event(UserEvent::Shortcut(request.body().clone()));
        })
        .build_as_child(window)
}

/// Calcula los bounds físicos del webview a partir del rect lógico que
/// egui dejó libre bajo sus paneles superiores.
pub fn bounds_from_content(
    content: egui::Rect,
    pixels_per_point: f32,
    window_size: winit::dpi::PhysicalSize<u32>,
) -> Rect {
    let top = (content.min.y * pixels_per_point).round() as i32;
    Rect {
        position: Position::Physical(PhysicalPosition::new(0, top)),
        size: Size::Physical(PhysicalSize::new(
            window_size.width,
            window_size.height.saturating_sub(top.max(0) as u32),
        )),
    }
}

/// ¿Tiene el rect un tamaño utilizable?
pub fn rect_is_sane(rect: &Rect) -> bool {
    matches!(rect.size, Size::Physical(ref s) if s.width > 0 && s.height > 0)
}
