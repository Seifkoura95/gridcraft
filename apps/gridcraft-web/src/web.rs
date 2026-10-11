//! The browser shell: web `Services`, drag-and-drop, and the eframe web runner.

use gridcraft_engine::Session;
use gridcraft_ui_egui::{Inbox, Services, SheetApp};
use wasm_bindgen::JsCast as _;

#[cfg(not(feature = "simplehub"))]
const CANVAS_ID: &str = "gridcraft_canvas";
#[cfg(not(feature = "simplehub"))]
const LOADING_ID: &str = "gridcraft_loading";

#[cfg(not(feature = "simplehub"))]
pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("no document");
            return;
        };
        let Some(canvas) = document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            log::error!("missing <canvas id=\"{CANVAS_ID}\">");
            return;
        };
        let mut options = eframe::WebOptions::default();
        if query().contains("webgl")
            && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
        {
            create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(move |cc| {
                    let inbox: Inbox = Inbox::default();
                    let mut session = Session::new();
                    let q = query();
                    if let Some(name) = q.split(['?', '&']).find_map(|kv| kv.strip_prefix("sample=")) {
                        let _ = session.execute("file.new", serde_json::json!({"sample": name}));
                    } else if q.contains("sample") {
                        let _ = session.execute("file.new", serde_json::json!({"sample": "sales"}));
                    }
                    let app = SheetApp::new(session, services(inbox.clone(), cc.egui_ctx.clone()));
                    SheetApp::setup_context(&cc.egui_ctx, false);
                    Ok(Box::new(WebShell { app, inbox, theme: Default::default() }))
                }),
            )
            .await;
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!("<p>{} failed to start: {e:?}</p><p>A browser with WebGPU or WebGL2 is required.</p>", gridcraft_ui_egui::APP_NAME)),
            }
        }
    });
}

#[cfg(not(feature = "simplehub"))]
fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}

/// A theme change requested from outside the frame loop (SimpleHub's `set_theme`). `dark` is
/// taken by the shell on its next frame; `ctx` lets the requester ask for that frame.
#[derive(Default)]
struct ThemeLink {
    dark: std::cell::Cell<Option<bool>>,
    ctx: std::cell::RefCell<Option<egui::Context>>,
}

impl ThemeLink {
    #[cfg_attr(not(feature = "simplehub"), allow(dead_code))]
    fn request(&self, dark: bool) {
        self.dark.set(Some(dark));
        if let Some(ctx) = self.ctx.borrow().as_ref() {
            ctx.request_repaint();
        }
    }
}

struct WebShell {
    app: SheetApp,
    inbox: Inbox,
    theme: std::rc::Rc<ThemeLink>,
}

impl eframe::App for WebShell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(dark) = self.theme.dark.take() {
            // `SheetApp::logic` re-applies the tokens when `dark` differs from the context.
            self.app.ui.dark = dark;
        }
        let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
        for f in dropped {
            let inbox = self.inbox.clone();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let name = f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dropped.xlsx".into());
                match f.bytes_async().await {
                    Ok(bytes) => {
                        inbox.lock().unwrap_or_else(|e| e.into_inner()).push((name, bytes));
                        ctx.request_repaint();
                    }
                    Err(e) => log::error!("couldn't read dropped file {name}: {e}"),
                }
            });
        }
        self.app.logic(ctx);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.app.raw_input_hook(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
    }
}

#[cfg(not(feature = "simplehub"))]
fn services(inbox: Inbox, ctx: egui::Context) -> Services {
    let open_inbox = inbox.clone();
    Services {
        open_async: Some(Box::new(move || {
            let inbox = open_inbox.clone();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let Some(file) =
                    rfd::AsyncFileDialog::new().add_filter("Spreadsheets", &["xlsx", "xlsm", "csv", "tsv", "txt", "json"]).pick_file().await
                else {
                    return;
                };
                let bytes = file.read().await;
                inbox.lock().unwrap_or_else(|e| e.into_inner()).push((file.file_name(), bytes));
                ctx.request_repaint();
            });
        })),
        download: Some(Box::new(|name: &str, bytes: &[u8]| {
            if let Err(e) = download(name, bytes) {
                log::error!("download of {name} failed: {e}");
            }
        })),
        open_url: Some(Box::new(|url: &str| {
            if let Some(w) = web_sys::window() {
                let _ = w.open_with_url_and_target(url, "_blank");
            }
        })),
        inbox: Some(inbox),
        ..Default::default()
    }
}

#[cfg(not(feature = "simplehub"))]
/// Triggers a browser download of `bytes`.
fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let js = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(if name.ends_with(".csv") { "text/csv" } else { "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" });
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(js)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(js)?.dyn_into().map_err(|_| "not an anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.style().set_property("display", "none").map_err(js)?;
    let body = document.body().ok_or("no body")?;
    body.append_child(&a).map_err(js)?;
    a.click();
    a.remove();
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        web_sys::Url::revoke_object_url(&url).ok();
    });
    window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000).map_err(js)?;
    Ok(())
}

/// Simple Sheets inside SimpleHub. The page mounts the app on a canvas it owns and hands over a
/// bridge object (`open(): Promise<{name, bytes}|null>`, `save(name, bytes): Promise<void>`) that
/// moves workbooks to and from Simple Cloud; nothing here touches the browser's file picker.
#[cfg(feature = "simplehub")]
mod simplehub {
    use super::{Inbox, Services, Session, SheetApp, ThemeLink, WebShell};
    use std::rc::Rc;
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    /// A running instance. SimpleHub calls `destroy` when the module unmounts.
    #[wasm_bindgen]
    pub struct SheetsHandle {
        runner: eframe::WebRunner,
        theme: Rc<ThemeLink>,
    }

    #[wasm_bindgen]
    impl SheetsHandle {
        pub fn destroy(&self) {
            self.runner.destroy();
        }

        pub fn has_panicked(&self) -> bool {
            self.runner.has_panicked()
        }

        /// Switches the running app to `"dark"` or `"light"` on its next frame. Other values are
        /// ignored.
        pub fn set_theme(&self, theme: &str) {
            if let Some(dark) = parse_theme(theme) {
                self.theme.request(dark);
            }
        }
    }

    /// `"dark"` turns Simple Sheets' dark mode on, `"light"` turns it off.
    fn parse_theme(theme: &str) -> Option<bool> {
        match theme.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(true),
            "light" => Some(false),
            _ => None,
        }
    }

    /// Starts Simple Sheets on `canvas`, with `bridge` providing open and save. An optional
    /// `theme` property (`"dark"` or `"light"`) on the bridge picks the initial theme; without
    /// it the app keeps its default (light).
    #[wasm_bindgen]
    pub async fn start_simple_sheets(canvas: web_sys::HtmlCanvasElement, bridge: JsValue) -> Result<SheetsHandle, JsValue> {
        eframe::WebLogger::init(log::LevelFilter::Info).ok();
        let dark = js_sys::Reflect::get(&bridge, &JsValue::from_str("theme"))
            .ok()
            .and_then(|v| v.as_string())
            .and_then(|s| parse_theme(&s))
            .unwrap_or(false);
        let theme = Rc::new(ThemeLink::default());
        let link = theme.clone();
        let runner = eframe::WebRunner::new();
        runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    let inbox: Inbox = Inbox::default();
                    let mut app = SheetApp::new(Session::new(), services(bridge, inbox.clone(), cc.egui_ctx.clone()));
                    app.ui.dark = dark;
                    SheetApp::setup_context(&cc.egui_ctx, dark);
                    *link.ctx.borrow_mut() = Some(cc.egui_ctx.clone());
                    Ok(Box::new(WebShell { app, inbox, theme: link }))
                }),
            )
            .await?;
        Ok(SheetsHandle { runner, theme })
    }

    fn services(bridge: JsValue, inbox: Inbox, ctx: egui::Context) -> Services {
        let open_bridge = bridge.clone();
        let open_inbox = inbox.clone();
        Services {
            open_async: Some(Box::new(move || {
                let bridge = open_bridge.clone();
                let inbox = open_inbox.clone();
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match call_open(&bridge).await {
                        Ok(Some((name, bytes))) => {
                            inbox.lock().unwrap_or_else(|e| e.into_inner()).push((name, bytes));
                            ctx.request_repaint();
                        }
                        Ok(None) => {}
                        Err(e) => log::error!("SimpleHub open failed: {e:?}"),
                    }
                });
            })),
            download: Some(Box::new(move |name: &str, bytes: &[u8]| {
                let bridge = bridge.clone();
                let name = name.to_string();
                let bytes = js_sys::Uint8Array::from(bytes);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = call_save(&bridge, &name, &bytes).await {
                        log::error!("SimpleHub save of {name} failed: {e:?}");
                    }
                });
            })),
            open_url: Some(Box::new(|url: &str| {
                if let Some(w) = web_sys::window() {
                    let _ = w.open_with_url_and_target(url, "_blank");
                }
            })),
            inbox: Some(inbox),
            ..Default::default()
        }
    }

    fn method(bridge: &JsValue, name: &str) -> Result<js_sys::Function, JsValue> {
        js_sys::Reflect::get(bridge, &JsValue::from_str(name))?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| JsValue::from_str(&format!("bridge.{name} is not a function")))
    }

    async fn call_open(bridge: &JsValue) -> Result<Option<(String, Vec<u8>)>, JsValue> {
        let promise: js_sys::Promise = method(bridge, "open")?.call0(bridge)?.dyn_into()?;
        let result = JsFuture::from(promise).await?;
        if result.is_null() || result.is_undefined() {
            return Ok(None);
        }
        let name = js_sys::Reflect::get(&result, &JsValue::from_str("name"))?.as_string().unwrap_or_else(|| "Book.xlsx".into());
        let bytes: js_sys::Uint8Array = js_sys::Reflect::get(&result, &JsValue::from_str("bytes"))?.dyn_into()?;
        Ok(Some((name, bytes.to_vec())))
    }

    async fn call_save(bridge: &JsValue, name: &str, bytes: &js_sys::Uint8Array) -> Result<(), JsValue> {
        let ret = method(bridge, "save")?.call2(bridge, &JsValue::from_str(name), bytes)?;
        if let Ok(promise) = ret.dyn_into::<js_sys::Promise>() {
            JsFuture::from(promise).await?;
        }
        Ok(())
    }
}
