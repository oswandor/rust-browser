# Rust Browser

Navegador de escritorio experimental escrito en **Rust**. Combina una interfaz
nativa construida con `egui` y renderizada con `wgpu` con un navegador real
embebido mediante `wry` y **WebView2** (Chromium) en Windows.

El objetivo es didáctico: mostrar una integración práctica entre una UI GPU,
una ventana nativa y un motor web, sin usar un framework de aplicación completo
como `eframe`.

![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust)
![Windows](https://img.shields.io/badge/plataforma-Windows%2010%2F11-0078D4?logo=windows)
![WebView2](https://img.shields.io/badge/motor-WebView2%20%2F%20Chromium-4285F4?logo=googlechrome&logoColor=white)

## Índice

- [Qué incluye](#qué-incluye)
- [Requisitos](#requisitos)
- [Instalación y ejecución](#instalación-y-ejecución)
- [Ejemplo: realizar una búsqueda](#ejemplo-realizar-una-búsqueda)
- [Controles y atajos](#controles-y-atajos)
- [Cómo se interpreta la barra de direcciones](#cómo-se-interpreta-la-barra-de-direcciones)
- [Arquitectura](#arquitectura)
- [Estructura del código](#estructura-del-código)
- [Datos locales](#datos-locales)
- [Limitaciones](#limitaciones-conocidas)

## Qué incluye

- Barra de direcciones que acepta URLs y texto para buscar con DuckDuckGo.
- Pestañas: crear, cerrar y alternar entre ellas.
- Navegación atrás, adelante, recarga e inicio.
- Atajos de teclado que funcionan incluso cuando el foco está dentro de la página.
- Título de la página y estado de carga en la pestaña activa.
- Historial persistente local, limitado a 5 000 entradas.
- Interfaz oscura con iconos vectoriales locales.

## Requisitos

| Componente | Necesario para |
| --- | --- |
| Windows 10 u 11 | Ejecutar la aplicación. |
| [Rust stable](https://www.rust-lang.org/tools/install) (MSRV 1.95+) | Compilar y ejecutar desde el código fuente. |
| Microsoft Edge WebView2 Runtime | Mostrar páginas web mediante Chromium. Normalmente ya viene instalado en Windows 10/11. |

Comprueba que Rust esté disponible con:

```powershell
rustc --version
cargo --version
```

Si la aplicación no abre contenido web, instala o actualiza el
[WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).

## Instalación y ejecución

Clona el repositorio y sitúate en su carpeta:

```powershell
git clone https://github.com/oswandor/rust-browser.git
cd rust-browser
```

Ejecuta en modo de desarrollo:

```powershell
cargo run
```

Para una compilación optimizada:

```powershell
cargo run --release
```

Para verificar la lógica que no requiere interfaz gráfica:

```powershell
cargo test
```

La primera compilación puede tardar unos minutos porque descarga y compila las
dependencias de la UI y de WebView2. Las siguientes ejecuciones aprovechan la
caché de Cargo.

## Ejemplo: realizar una búsqueda

Este ejemplo muestra el uso más directo del navegador: escribir una consulta,
sin tener que abrir previamente un buscador.

1. Abre la aplicación con `cargo run`.
2. Haz clic en la barra que dice **“Busca o escribe una URL…”** o presiona
   <kbd>Ctrl</kbd> + <kbd>L</kbd>.
3. Escribe `rust web framework`.
4. Presiona <kbd>Enter</kbd>.

La aplicación reconoce que el texto no es una dirección y abre DuckDuckGo con
esta URL:

```text
https://duckduckgo.com/?q=rust+web+framework
```

La pestaña muestra un indicador de carga mientras la página responde y después
adopta el título entregado por el sitio. Para volver a la página anterior usa
el botón **←** o <kbd>Alt</kbd> + <kbd>←</kbd>.

> **Captura pendiente.** La ubicación prevista para este ejemplo es
> `assets/screenshots/busqueda-duckduckgo.png`. En esta ejecución el compositor
> de Windows no expuso el contenido de la ventana a la herramienta de captura;
> se dejó esta referencia para añadir una captura real desde una sesión gráfica.

## Controles y atajos

| Acción | Interfaz | Atajo |
| --- | --- | --- |
| Nueva pestaña | Botón `+` | <kbd>Ctrl</kbd> + <kbd>T</kbd> |
| Cerrar pestaña activa | Botón `×` | <kbd>Ctrl</kbd> + <kbd>W</kbd> |
| Enfocar la barra de direcciones | Barra de direcciones | <kbd>Ctrl</kbd> + <kbd>L</kbd> |
| Atrás | Botón `←` | <kbd>Alt</kbd> + <kbd>←</kbd> |
| Adelante | Botón `→` | <kbd>Alt</kbd> + <kbd>→</kbd> |
| Recargar | Botón `⟳` | <kbd>F5</kbd> o <kbd>Ctrl</kbd> + <kbd>R</kbd> |
| Ir al inicio | Botón `⌂` | — |

## Cómo se interpreta la barra de direcciones

La misma barra sirve para navegar y buscar. Estos son algunos casos útiles:

| Texto introducido | Acción resultante |
| --- | --- |
| `example.com` | Abre `https://example.com`. |
| `https://example.com/docs` | Conserva y abre la URL indicada. |
| `localhost:3000` | Abre `http://localhost:3000`. |
| `rust web framework` | Busca el texto en DuckDuckGo. |
| `about:blank` | Abre una pestaña en blanco. |

Solo se aceptan explícitamente los esquemas `http`, `https`, `file`, `about` y
`data`. El resto del texto se trata como una búsqueda; esto evita que una cadena
ambigua se interprete erróneamente como una URL.

## Arquitectura

```text
┌─────────────────────────────────────────────────────────┐
│ Ventana winit 0.30 (ApplicationHandler<UserEvent>)       │
│                                                         │
│  egui 0.36 ──► wgpu 30 / Direct3D 12                    │
│  pestañas, toolbar y barra de direcciones                │
│                                                         │
│  wry 0.57 ──► WebView2 / Chromium                       │
│  contenido web hijo dentro del área libre de la ventana  │
└─────────────────────────────────────────────────────────┘
                 ▲                         │
                 └── EventLoopProxy ───────┘
```

En cada frame, `egui` dibuja los paneles superiores y emite acciones de usuario.
Después, la aplicación actualiza el estado de Rust y ordena al `WebView` navegar,
recargar o tomar el foco. El rectángulo libre de la interfaz se convierte a
píxeles físicos y se usa para ajustar los límites del WebView hijo.

Los eventos del WebView —fin de carga, cambio de título y atajos reenviados por
IPC— se devuelven al hilo principal con `EventLoopProxy<UserEvent>`.

### Decisiones relevantes

- **Un WebView compartido:** cada pestaña guarda su URL, título e historial en
  Rust; al alternar pestañas se vuelve a cargar su URL actual. Es una decisión
  deliberada para simplificar el proyecto.
- **wgpu en lugar de OpenGL:** el renderizado usa Direct3D 12, una ruta más
  consistente en el entorno Windows donde también vive WebView2.
- **Atajos por IPC:** WebView2 captura el teclado al navegar. Un script de
  inicialización intercepta los atajos y los reenvía a la aplicación nativa.
- **Historial en JSON:** permite inspeccionarlo y borrarlo manualmente sin una
  base de datos adicional.

## Estructura del código

| Archivo | Responsabilidad |
| --- | --- |
| `src/main.rs` | Ventana, superficie `wgpu` y ciclo de eventos. |
| `src/app.rs` | Estado del navegador, acciones, eventos de WebView y coordinación general. |
| `src/ui.rs` | Dibujo de pestañas, toolbar y barra de direcciones con egui. |
| `src/webview.rs` | Creación del WebView hijo, IPC de atajos y cálculo de límites físicos. |
| `src/navigation.rs` | Modelo de pestaña, historial por pestaña y normalización de direcciones. |
| `src/history.rs` | Lectura y escritura del historial persistente. |
| `assets/icons.ttf` | Subconjunto de Material Symbols Outlined usado por la UI. |

## Datos locales

El historial global se guarda en:

```text
%APPDATA%\rust-browser\history.json
```

No se sincroniza con ninguna cuenta ni se transmite desde la aplicación. Si
eliminas ese archivo con el navegador cerrado, el historial se reiniciará.

## Limitaciones conocidas

- Cambiar de pestaña vuelve a cargar la página y se pierde el estado temporal
  de JavaScript o la posición de desplazamiento.
- No incluye marcadores, descargas ni favicones.
- La barra de direcciones no selecciona automáticamente todo el texto al ganar
  el foco.
- Los enlaces con `target="_blank"` no se abren aún en una pestaña nueva.

## Licencias y atribuciones

Los iconos de `assets/icons.ttf` proceden de Material Symbols Outlined de Google,
con licencia Apache 2.0, y fueron reducidos a los glifos usados por la interfaz.
