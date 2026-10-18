//! Estado del navegador y bucle de eventos (winit `ApplicationHandler`).

use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use wry::WebView;

use crate::history::History;
use crate::navigation::{Tab, HOME_URL, NEW_TAB_URL};
use crate::WgpuWindowContext;

/// Eventos que llegan al hilo principal desde los callbacks de wry
/// (que pueden ejecutarse en otro hilo) o desde el propio egui.
#[derive(Debug, Clone)]
pub enum UserEvent {
    /// Retardo de repaint solicitado por egui.
    Redraw(Duration),
    PageLoadStarted(String),
    PageLoadFinished(String),
    Title(String),
    /// Atajo enviado por el script inyectado en la página (IPC).
    Shortcut(String),
}

/// Acciones producidas por la UI en un frame, ejecutadas tras pintar.
#[derive(Debug)]
pub enum Action {
    Navigate(String),
    Back,
    Forward,
    Reload,
    Home,
    NewTab,
    CloseTab(usize),
    SwitchTab(usize),
    FocusAddress,
    /// Devolver el foco del teclado a la página.
    FocusPage,
}

/// Estado que la UI (egui) lee y escribe cada frame.
pub struct BrowserState {
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// Texto de la barra de direcciones (la URL en curso o lo que el usuario teclea).
    pub address: String,
    /// Pendiente de dar foco a la barra de direcciones en el próximo frame.
    pub focus_address: bool,
    /// Bounds (píxeles físicos) que debería ocupar el webview tras este frame.
    pub desired_webview_bounds: Option<wry::Rect>,
    /// Últimos bounds aplicados al webview.
    pub current_bounds: wry::Rect,
    pub history: History,
}

impl BrowserState {
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }
    pub fn is_loading(&self) -> bool {
        self.tabs.iter().any(|t| t.loading)
    }
}

pub struct BrowserApp {
    pub proxy: EventLoopProxy<UserEvent>,
    ctx: egui::Context,
    wgpu: Option<WgpuWindowContext>,
    repaint_delay: Duration,
    browser: BrowserState,
    webview: Option<WebView>,
}

impl BrowserApp {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let ctx = egui::Context::default();

        // Fuente de iconos Material Symbols (sub-set de 11 KB en assets/).
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "material-icons".into(),
            std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/icons.ttf"
            ))),
        );
        fonts.families.insert(
            egui::FontFamily::Name("material".into()),
            vec!["material-icons".into()],
        );
        ctx.set_fonts(fonts);

        let repaint_proxy = egui::mutex::Mutex::new(proxy.clone());
        ctx.set_request_repaint_callback(move |info| {
            let _ = repaint_proxy.lock().send_event(UserEvent::Redraw(info.delay));
        });
        Self {
            proxy,
            ctx,
            wgpu: None,
            repaint_delay: Duration::MAX,
            browser: BrowserState {
                tabs: vec![Tab::new(HOME_URL)],
                active: 0,
                address: HOME_URL.to_string(),
                focus_address: false,
                desired_webview_bounds: None,
                current_bounds: wry::Rect::default(),
                history: History::load(),
            },
            webview: None,
        }
    }

    /// Ejecuta una acción de UI contra el estado + el webview.
    fn apply(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        let Some(webview) = self.webview.as_ref() else {
            return;
        };
        let st = &mut self.browser;
        match action {
            Action::Navigate(url) => {
                st.active_tab_mut().push(url.clone());
                st.active_tab_mut().loading = true;
                st.address = url.clone();
                let _ = webview.load_url(&url);
            }
            Action::Back => {
                if let Some(url) = st.active_tab_mut().back() {
                    st.active_tab_mut().loading = true;
                    st.address = url.clone();
                    let _ = webview.load_url(&url);
                }
            }
            Action::Forward => {
                if let Some(url) = st.active_tab_mut().forward() {
                    st.active_tab_mut().loading = true;
                    st.address = url.clone();
                    let _ = webview.load_url(&url);
                }
            }
            Action::Reload => {
                st.active_tab_mut().loading = true;
                let _ = webview.reload();
            }
            Action::Home => {
                self.apply(Action::Navigate(HOME_URL.to_string()), event_loop);
            }
            Action::NewTab => {
                st.tabs.push(Tab::new(NEW_TAB_URL));
                st.active = st.tabs.len() - 1;
                st.address.clear();
                st.active_tab_mut().loading = true;
                st.focus_address = true;
                let _ = webview.load_url(NEW_TAB_URL);
                // El foco de teclado está en el webview: devolverlo a la ventana.
                let _ = webview.focus_parent();
            }
            Action::CloseTab(i) => {
                if st.tabs.len() <= 1 {
                    self.exit(event_loop);
                    return;
                }
                st.tabs.remove(i);
                if i < st.active {
                    st.active -= 1;
                } else {
                    st.active = st.active.min(st.tabs.len() - 1);
                }
                let url = st.active_tab().url().to_string();
                st.address = display_address(&url);
                let _ = webview.load_url(&url);
            }
            Action::SwitchTab(i) => {
                if i != st.active && i < st.tabs.len() {
                    st.active = i;
                    let url = st.active_tab().url().to_string();
                    st.address = display_address(&url);
                    st.active_tab_mut().loading = true;
                    let _ = webview.load_url(&url);
                }
            }
            Action::FocusAddress => {
                st.focus_address = true;
                let _ = webview.focus_parent();
            }
            Action::FocusPage => {
                let _ = webview.focus();
            }
        }
    }

    fn exit(&mut self, event_loop: &ActiveEventLoop) {
        self.browser.history.save();
        event_loop.exit();
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let mut actions = Vec::new();

        {
            let wgpu = self.wgpu.as_mut().expect("redraw antes de resumed");
            let browser = &mut self.browser;
            wgpu.render(&self.ctx, browser, &mut actions);
        }

        for action in actions {
            self.apply(action, event_loop);
        }

        // Sincronizar el área del webview con lo que egui dejó libre.
        if let Some(desired) = self.browser.desired_webview_bounds {
            if desired != self.browser.current_bounds
                && crate::webview::rect_is_sane(&desired)
            {
                if let Some(webview) = self.webview.as_ref() {
                    if webview.set_bounds(desired).is_ok() {
                        self.browser.current_bounds = desired;
                    }
                }
            }
        }

        // Mientras carga algo, animar (punto de carga en la pestaña).
        if self.browser.is_loading() {
            self.repaint_delay = self.repaint_delay.min(Duration::from_millis(150));
        }
        let window = self.wgpu.as_ref().expect("wgpu").window();
        event_loop.set_control_flow(if self.repaint_delay.is_zero() {
            window.request_redraw();
            ControlFlow::Poll
        } else if let Some(deadline) = Instant::now().checked_add(self.repaint_delay) {
            ControlFlow::WaitUntil(deadline)
        } else {
            ControlFlow::Wait
        });
    }
}

impl ApplicationHandler<UserEvent> for BrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let wgpu = WgpuWindowContext::new(event_loop, self.ctx.clone());

        // Webview hijo bajo una estimación inicial de la barra de pestañas+toolbar
        // (38 + 46 px lógicos); el primer frame de egui corrige los bounds reales.
        let scale = wgpu.window().scale_factor() as f32;
        let estimated_top = (80.0 * scale).round() as i32;
        let size = wgpu.window().inner_size();
        let bounds = wry::Rect {
            position: wry::dpi::Position::Physical(
                wry::dpi::PhysicalPosition::new(0, estimated_top),
            ),
            size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                size.width,
                size.height.saturating_sub(estimated_top as u32),
            )),
        };
        let webview =
            crate::webview::create(wgpu.window(), &self.proxy, HOME_URL, bounds)
                .expect("crear WebView2");

        wgpu.window().request_redraw();

        self.wgpu = Some(wgpu);
        self.webview = Some(webview);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            self.exit(event_loop);
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            self.redraw(event_loop);
            return;
        }
        if let WindowEvent::Resized(physical_size) = &event {
            if let Some(wgpu) = self.wgpu.as_mut() {
                wgpu.resize(*physical_size);
            }
        }
        if let Some(wgpu) = self.wgpu.as_mut() {
            let response = wgpu.on_window_event(&event);
            if response.repaint {
                wgpu.window().request_redraw();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Redraw(delay) => self.repaint_delay = delay,
            UserEvent::PageLoadStarted(url) => {
                let st = &mut self.browser;
                st.active_tab_mut().loading = true;
                if url != NEW_TAB_URL {
                    st.address = url;
                }
            }
            UserEvent::PageLoadFinished(url) => {
                let st = &mut self.browser;
                st.active_tab_mut().loading = false;
                st.active_tab_mut().replace_current(&url);
                if url == NEW_TAB_URL {
                    st.address.clear();
                } else {
                    st.address = url.clone();
                    let title = st.active_tab().title.clone();
                    st.history.record(&url, &title);
                }
            }
            UserEvent::Title(title) => {
                self.browser.active_tab_mut().title = title.clone();
                if let Some(wgpu) = self.wgpu.as_ref() {
                    wgpu.window()
                        .set_title(&format!("{title} — Rust Browser"));
                }
            }
            UserEvent::Shortcut(msg) => {
                let action = match msg.as_str() {
                    "ctrl-t" => Action::NewTab,
                    "ctrl-w" => Action::CloseTab(self.browser.active),
                    "ctrl-l" => Action::FocusAddress,
                    "reload" => Action::Reload,
                    "back" => Action::Back,
                    "forward" => Action::Forward,
                    _ => return,
                };
                self.apply(action, event_loop);
            }
        }
        if let Some(wgpu) = self.wgpu.as_ref() {
            wgpu.window().request_redraw();
        }
    }

    fn new_events(
        &mut self,
        _event_loop: &ActiveEventLoop,
        cause: winit::event::StartCause,
    ) {
        if let winit::event::StartCause::ResumeTimeReached { .. } = cause {
            if let Some(wgpu) = self.wgpu.as_ref() {
                wgpu.window().request_redraw();
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.browser.history.save();
    }
}

/// Qué mostrar en la barra de direcciones para una URL interna.
fn display_address(url: &str) -> String {
    if url == NEW_TAB_URL {
        String::new()
    } else {
        url.to_string()
    }
}
