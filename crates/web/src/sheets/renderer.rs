//! Turning a sheet file into pixels.
//!
//! Everything that shows a sheet goes through this one narrow interface — the viewer, the print
//! pack, and sheet slides. That is the point of it: the engine underneath is four megabytes of
//! Chrome's PDF renderer, and the interface is what keeps that decision reversible. Two
//! operations, page count and render a page, and no screen knows which engine answered.

use std::cell::RefCell;

use pdfium_render::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, ImageData};

/// Where the engine is fetched from. Copied into the distribution by the Trunk hook, never
/// imported, so nothing downloads it until a sheet is opened.
const ENGINE: &str = "/pdfium/pdfium.js";
const ENGINE_WASM: &str = "/pdfium/pdfium.wasm";

#[wasm_bindgen(inline_js = r#"
// The only JavaScript the sheet renderer needs, and it is here for two reasons that Rust cannot
// supply on its own.
//
// The dynamic import is what defers four megabytes of engine until the first sheet is opened.
// And `initialize_pdfium_render` is exported by pdfium-render to JavaScript only — it hands the
// engine's heap and function table to the Rust bindings, and it wants our own module's exports
// as its second argument. Trunk publishes those on `window.wasmBindings`, so this is six lines
// rather than a custom Trunk initialiser.
export async function bind_engine(loader, wasm) {
  const { PDFiumModule } = await import(loader);
  const engine = await PDFiumModule({ locateFile: () => wasm });
  const ours = window.wasmBindings;

  return ours.initialize_pdfium_render(engine, ours, false);
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn bind_engine(loader: &str, wasm: &str) -> Result<JsValue, JsValue>;
}

thread_local! {
    /// The engine, once per tab, deliberately leaked.
    ///
    /// Pdfium is a process-wide singleton and `pdfium-render` enforces it: the second call to
    /// `bind_to_system_library` returns "already initialized". So a `Pdfium` per render would
    /// work exactly once — which is how this was written first, and why attaching a sheet read
    /// its page count and then the viewer could not draw it.
    static ENGINE_HANDLE: RefCell<Option<&'static Pdfium>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum RenderError {
    #[error("the sheet renderer could not be fetched")]
    EngineMissing,
    #[error("the sheet renderer could not be bound")]
    EngineUnbound,
    #[error("the sheet renderer could not be reached")]
    EngineUnavailable,
    #[error("this file could not be read as a PDF")]
    Unreadable,
    #[error("that page is not in this file")]
    NoSuchPage,
}

/// One rendered page: raw RGBA, ready for a canvas.
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RenderedPage {
    /// The shape a canvas wants. Kept here so no screen has to know the pixel order.
    pub fn to_image_data(&self) -> Result<ImageData, JsValue> {
        ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(&self.pixels),
            self.width,
            self.height,
        )
    }
}

/// What every sheet surface needs, and nothing else.
pub trait SheetRenderer {
    fn page_count(&self, bytes: &[u8]) -> Result<i32, RenderError>;

    /// Renders one page, 0-based, at a target width in device pixels. The height follows from
    /// the page's own aspect ratio: a sheet that is not the shape it was engraved in is wrong.
    fn render(&self, bytes: &[u8], page: i32, width: u32) -> Result<RenderedPage, RenderError>;
}

/// The engine, once it is up.
pub struct PdfiumRenderer {
    pdfium: &'static Pdfium,
}

impl PdfiumRenderer {
    /// Fetches the engine if this tab has not already, and binds it to the Rust side.
    ///
    /// The binding is a call into `pdfium-render`'s own exported function, made from Rust rather
    /// than from a Trunk initialiser: `wasm_bindgen::exports()` is the same object the
    /// initialiser would have passed, and doing it here keeps the engine's arrival lazy and
    /// keeps the app out of JavaScript.
    pub async fn load() -> Result<PdfiumRenderer, RenderError> {
        if let Some(pdfium) = ENGINE_HANDLE.with(|held| *held.borrow()) {
            return Ok(PdfiumRenderer { pdfium });
        }

        let bound = bind_engine(ENGINE, ENGINE_WASM)
            .await
            .map_err(|_| RenderError::EngineMissing)?;

        if !bound.is_truthy() {
            return Err(RenderError::EngineUnbound);
        }

        let bindings =
            Pdfium::bind_to_system_library().map_err(|_| RenderError::EngineUnavailable)?;
        let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));

        ENGINE_HANDLE.with(|held| *held.borrow_mut() = Some(pdfium));

        Ok(PdfiumRenderer { pdfium })
    }
}

impl SheetRenderer for PdfiumRenderer {
    fn page_count(&self, bytes: &[u8]) -> Result<i32, RenderError> {
        let document = self
            .pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|_| RenderError::Unreadable)?;

        Ok(document.pages().len())
    }

    fn render(&self, bytes: &[u8], page: i32, width: u32) -> Result<RenderedPage, RenderError> {
        let document = self
            .pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|_| RenderError::Unreadable)?;

        let page = document
            .pages()
            .get(page)
            .map_err(|_| RenderError::NoSuchPage)?;

        let config = PdfRenderConfig::new().set_target_width(width as i32);
        let bitmap = page
            .render_with_config(&config)
            .map_err(|_| RenderError::Unreadable)?;

        Ok(RenderedPage {
            width: bitmap.width() as u32,
            height: bitmap.height() as u32,
            pixels: bitmap.as_rgba_bytes(),
        })
    }
}

/// The bytes of a stored file, which is what every caller actually has.
pub async fn bytes_of(blob: &Blob) -> Option<Vec<u8>> {
    let buffer = JsFuture::from(blob.array_buffer()).await.ok()?;

    Some(js_sys::Uint8Array::new(&buffer).to_vec())
}
