//! Getting the audience window onto the projector.
//!
//! Three mechanisms, strongest first, and the weakest one always works: the app degrades until
//! something is on screen rather than failing with a permission error at the moment a service
//! starts (presenter-output journey).

use js_sys::{Array, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// How the audience window got where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Placed on a named second screen by the Window Management API.
    WindowManagement,
    /// Handed to a receiver by the Presentation API.
    PresentationApi,
    /// A plain window, and an honest instruction.
    Manual,
}

pub struct Opened {
    pub route: Route,
    pub window: Option<web_sys::Window>,
    /// Set when the Presentation API is carrying the output instead of a window we opened.
    pub connection: Option<JsValue>,
    pub hint: Option<String>,
}

/// A prompt nobody answers must not hold up the start of a service.
const SCREEN_PROMPT_MS: u32 = 10_000;

/// Discovery that has not answered this fast is treated as "no receiver": a plain window now
/// beats a cast screen later.
const CAST_DISCOVERY_MS: u32 = 1_500;

async fn call(target: &JsValue, method: &str, args: &Array) -> Option<JsValue> {
    let function = Reflect::get(target, &JsValue::from_str(method))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;

    let answer = Reflect::apply(&function, target, args).ok()?;

    match answer.dyn_into::<js_sys::Promise>() {
        Ok(promise) => JsFuture::from(promise).await.ok(),
        Err(value) => Some(value),
    }
}

fn number(held: &JsValue, name: &str) -> f64 {
    Reflect::get(held, &JsValue::from_str(name))
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
}

/// The screen that is not the one the operator is sitting in front of.
///
/// `isExtended` is readable without any permission, so a laptop with one screen is never asked
/// for one: the prompt only appears when there is actually a second screen to put the audience
/// window on.
async fn external_screen() -> Option<JsValue> {
    let window = web_sys::window()?;
    let screen = window.screen().ok()?;

    if !Reflect::get(&screen, &JsValue::from_str("isExtended"))
        .map(|value| value.is_truthy())
        .unwrap_or(false)
    {
        return None;
    }

    // Raced with a timer: an unanswered permission prompt must not block the service.
    let details = race(
        call(window.as_ref(), "getScreenDetails", &Array::new()),
        SCREEN_PROMPT_MS,
    )
    .await??;

    let screens = Reflect::get(&details, &JsValue::from_str("screens")).ok()?;
    let screens = js_sys::Array::from(&screens);

    // Permission refused, or no second screen. Both fall through to the next mechanism.
    screens.iter().find(|screen| {
        !Reflect::get(screen, &JsValue::from_str("isPrimary")).is_ok_and(|value| value.is_truthy())
    })
}

/// Casting, but only when there is something to cast to.
///
/// `start()` opens the browser's device picker and waits for a person, so it must never be
/// called speculatively: an operator with no receiver on the network would get a dialog in front
/// of them at the moment a service starts, and the audience window would wait behind it.
async fn cast_to(url: &str) -> Option<JsValue> {
    let window = web_sys::window()?;
    let constructor = Reflect::get(&window, &JsValue::from_str("PresentationRequest"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;

    let request = Reflect::construct(
        &constructor,
        &Array::of1(&Array::of1(&JsValue::from_str(url))),
    )
    .ok()?;

    let availability = race(
        call(&request, "getAvailability", &Array::new()),
        CAST_DISCOVERY_MS,
    )
    .await??;

    if !Reflect::get(&availability, &JsValue::from_str("value"))
        .is_ok_and(|value| value.is_truthy())
    {
        return None;
    }

    // The user may still cancel the picker, which lands here as `None`.
    call(&request, "start", &Array::new()).await
}

/// Whichever mechanism gets a screen up first.
pub async fn open_audience(url: &str) -> Opened {
    let window = web_sys::window();

    if let (Some(window), Some(screen)) = (window.as_ref(), external_screen().await) {
        let features = format!(
            "left={},top={},width={},height={},noopener",
            number(&screen, "availLeft"),
            number(&screen, "availTop"),
            number(&screen, "availWidth"),
            number(&screen, "availHeight"),
        );

        if let Ok(Some(opened)) =
            window.open_with_url_and_target_and_features(url, "aurum-audience", &features)
        {
            let label = Reflect::get(&screen, &JsValue::from_str("label"))
                .ok()
                .and_then(|value| value.as_string())
                .unwrap_or_else(|| "the second screen".to_owned());

            return Opened {
                route: Route::WindowManagement,
                window: Some(opened),
                connection: None,
                hint: Some(format!(
                    "Opened on {label}. Press F in that window if it is not full screen.",
                )),
            };
        }
    }

    if let Some(connection) = cast_to(url).await {
        return Opened {
            route: Route::PresentationApi,
            window: None,
            connection: Some(connection),
            hint: None,
        };
    }

    // Nothing clever is available. A plain window, and an honest instruction.
    let opened = window.as_ref().and_then(|window| {
        window
            .open_with_url_and_target_and_features(url, "aurum-audience", "width=1280,height=720")
            .ok()
            .flatten()
    });

    Opened {
        route: Route::Manual,
        window: opened,
        connection: None,
        hint: Some("Drag this window to the projector and press F to go full screen.".to_owned()),
    }
}

/// Full screen from inside the output window, where the gesture requirement is satisfiable.
pub fn go_fullscreen() {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
    {
        let _ = call_sync(&element, "requestFullscreen");
    }
}

fn call_sync(target: &JsValue, method: &str) -> Option<JsValue> {
    let function = Reflect::get(target, &JsValue::from_str(method))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;

    Reflect::apply(&function, target, &Array::new()).ok()
}

/// The first of a promise and a timer. `None` means the timer won.
async fn race<T>(work: impl std::future::Future<Output = T>, within: u32) -> Option<T> {
    use futures_util::future::{Either, select};

    let timer = gloo_timers::future::TimeoutFuture::new(within);

    match select(Box::pin(work), timer).await {
        Either::Left((answer, _)) => Some(answer),
        Either::Right(_) => None,
    }
}
