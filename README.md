# rust-browser

Navegador web liviano escrito en **Rust** como proyecto de aprendizaje: interfaz
nativa (pestañas + barra de direcciones) con **egui** renderizado vía **wgpu**
(Direct3D 12 en Windows) y contenido web real gracias a **wry**, que embebe el
motor **WebView2 / Chromium** del sistema.

![stack](https://img.shields.io/badge/egui-0.36-orange) ![stack](https://img.shields.io/badge/wgpu-30-blue) ![stack](https://img.shields.io/badge/wry-0.57-green)

## Ejecutar

```bash
cargo run        # debug (con consola)
cargo run -r     # release
cargo test       # tests unitarios (normalización de URLs, historial de pestañas)
```

Requisitos: Rust stable (MSRV 1.95+) y el WebView2 Runtime (preinstalado en
Windows 10/11).

## Interfaz

Estilo oscuro inspirado en Edge/Chrome:

- Pestañas de altura moderada con esquinas superiores redondeadas; la pestaña
  activa comparte color con la toolbar y se funde con ella.
- Iconos vectoriales **Material Symbols Outlined** (sub-set de 11 KB en
  `assets/icons.ttf`, licencia Apache 2.0; generado con
  `fonttools subset` a partir de los iconos de Google).
- Barra de direcciones en píldora con candado en HTTPS.
- Indicador de carga (spinner) dentro de la pestaña activa.

## Funcionalidades

- Barra de direcciones inteligente: `example.com` → `https://example.com`,
  texto libre → búsqueda en DuckDuckGo, `localhost:3000` → `http://…`.
- Pestañas múltiples: crear (botón `+` / Ctrl+T), cerrar (`×` / Ctrl+W),
  cambiar con click.
- Navegación: Atrás/Adelante (botones, Alt+←/→), Recargar (⟳, F5, Ctrl+R),
  Inicio (⌂).
- Atajos funcionando **dentro de las páginas** (Ctrl+T, Ctrl+W, Ctrl+L, F5,
  Alt+←/→) mediante un script inyectado + IPC.
- Título de página en ventana y pestaña, indicador de carga (●) en la pestaña.
- Historial persistente en `%APPDATA%\rust-browser\history.json` (máx. 5000
  entradas, se guarda al navegar y al salir).

## Arquitectura

```
┌──────────────────────────────────────────────────────┐
│ Ventana winit 0.30 (ApplicationHandler<UserEvent>)   │
│                                                      │
│ ┌──────────────────────────────────────────────────┐ │
│ │ egui 0.36 (Panel::top ×2)                        │ │ ← UI nativa Rust
│ │  · barra de pestañas   · toolbar (← → ⟳ ⌂ + URL)│ │   renderizada con
│ └──────────────────────────────────────────────────┘ │   wgpu → D3D12
│ ┌──────────────────────────────────────────────────┐ │
│ │ wry 0.57 → WebView2 hijo (bounds = área libre) 	│ │ ← Motor Chromium
│ │  · on_page_load · title_changed · ipc_handler  	│ │   del sistema
│ └──────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┘
         ▲                                   │
         └──── EventLoopProxy<UserEvent> ────┘
```

Flujo por frame (`app.rs::redraw`):

1. `egui_winit::State::take_egui_input` → `ctx.run_ui` pinta los paneles y
   produce `Vec<Action>` (Navigate, NewTab, Back…).
2. Las acciones se aplican tras pintar: mutan `BrowserState` y ordenan
   `webview.load_url/reload/focus…`.
3. El rect lógico que egui deja libre bajo los paneles se convierte a píxeles
   físicos (`wry::Rect`) y se aplica con `webview.set_bounds` solo si cambió.
4. Los callbacks de wry (carga, título, IPC de atajos) corren fuera del hilo
   principal en algunos casos: envían `UserEvent` por `EventLoopProxy` y el
   handler los aplica de forma thread-safe.

### Decisiones de diseño

- **Un solo WebView compartido.** El estado de cada pestaña (historial, cursor,
  título) vive en Rust (`navigation::Tab`); cambiar de pestaña vuelve a cargar
  su URL actual. Trade-off documentado: la pestaña de fondo pierde su estado
  JS/scroll al volver (fase 2: un webview por pestaña).
- **wgpu en vez de OpenGL.** El ejemplo oficial `pure_glow` (glutin/WGL) se
  probó en la máquina de desarrollo y la superficie nunca presenta (pantalla
  negra), síntoma típico de sesiones remotas/GPU híbrida con WGL. wgpu usa
  D3D12 — el mismo camino que WebView2 — y presenta correctamente.
- **Atajos vía JS inyectado.** El HWND hijo del WebView2 captura el teclado,
  así que winit nunca ve Ctrl+T mientras se navega. Un `keydown` listener
  (script de inicialización de wry) reenvía los atajos por `window.ipc.postMessage`.
- **Historial write-through en JSON** con `serde`: simple de inspeccionar a
  mano; sin base de datos para no oscurecer el aprendizaje.

### Módulos

| Archivo | Responsabilidad |
|---|---|
| `src/main.rs` | `WgpuWindowContext`: ventana, surface wgpu, encoder, `egui_wgpu::Renderer` |
| `src/app.rs` | `BrowserApp`: estado, `ApplicationHandler`, acciones, eventos del webview |
| `src/ui.rs` | Dibujo egui: barra de pestañas + toolbar, atajos con foco en la UI |
| `src/webview.rs` | Creación del webview hijo, script de atajos, cálculo de bounds |
| `src/navigation.rs` | `Tab` (historial/URL), normalización de direcciones (+ tests) |
| `src/history.rs` | Historial global persistente en JSON |

## Limitaciones conocidas (fase 2)

- Cambiar de pestaña recarga la página (un webview por pestaña lo arreglaría).
- No hay marcadores, descargas ni favicones.
- La barra de direcciones no autoselecciona su texto al ganar foco.
- Al hacer click en un enlace con `target="_blank"` no se abre nueva pestaña.

## Aprendizajes del proyecto

- Integración egui + winit sin eframe (control total del event loop).
- wgpu 30: `CurrentSurfaceTexture`, `queue.present()`, `forget_lifetime()`.
- wry 0.57: `build_as_child`, bounds físicos con tipos `dpi`, IPC bidireccional.
- El truco de mezclar UI GPU con ventana nativa hija: quien cubre a quién, y
  por qué los bounds se recalculan cada frame.
