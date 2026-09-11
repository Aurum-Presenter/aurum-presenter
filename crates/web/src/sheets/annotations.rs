//! Freehand marks over a sheet.
//!
//! Coordinates are normalised to 0–1 of the page, never pixels (business rule 7). A mark drawn on
//! a phone at fit-width has to land in the same place on a tablet at 200 % and on a rotated page,
//! and the only way that holds is to store where it is on the *page* rather than on the screen.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use web_sys::{Element, PointerEvent};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Stroke {
    pub kind: String,
    pub color: String,
    pub width: f64,
    pub points: Vec<(f64, f64)>,
}

const COLORS: [&str; 5] = ["#dc2626", "#2563eb", "#16a34a", "#eab308", "#0f172a"];

/// A stroke as an SVG path, scaled from the page back onto the pixels on screen.
fn path_of(stroke: &Stroke, width: f64, height: f64) -> String {
    stroke
        .points
        .iter()
        .enumerate()
        .map(|(index, (x, y))| {
            let command = if index == 0 { "M" } else { "L" };

            format!("{command} {} {}", x * width, y * height)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where a pointer is on the page, as a fraction of it.
fn at(event: &PointerEvent) -> Option<(f64, f64)> {
    let box_of = event
        .current_target()?
        .dyn_into::<Element>()
        .ok()?
        .get_bounding_client_rect();

    Some((
        ((event.client_x() as f64 - box_of.left()) / box_of.width()).clamp(0.0, 1.0),
        ((event.client_y() as f64 - box_of.top()) / box_of.height()).clamp(0.0, 1.0),
    ))
}

#[component]
pub fn AnnotationLayer(
    width: Signal<f64>,
    height: Signal<f64>,
    /// Everything visible: the reader's own marks plus anything shared with the band.
    strokes: Signal<Vec<Stroke>>,
    /// The subset this reader owns in the current scope, which is what editing replaces.
    mine: Signal<Vec<Stroke>>,
    drawing: Signal<bool>,
    on_change: Callback<Vec<Stroke>>,
) -> impl IntoView {
    let color = RwSignal::new(COLORS[0].to_owned());
    let thickness = RwSignal::new(3.0_f64);
    let current = RwSignal::new(None::<Stroke>);

    let drawn = Signal::derive(move || {
        let mut all = strokes.get();

        all.extend(current.get());
        all
    });

    view! {
        <svg
            class=move || if drawing.get() {
                "absolute inset-0 cursor-crosshair touch-none"
            } else {
                "absolute inset-0 pointer-events-none"
            }
            data-testid="annotations"
            width=move || width.get()
            height=move || height.get()
            on:pointerdown=move |event: PointerEvent| {
                if !drawing.get_untracked() {
                    return;
                }

                if let Some(target) = event
                    .current_target()
                    .and_then(|target| target.dyn_into::<Element>().ok())
                {
                    // Captured so a stroke that leaves the page still ends where it ended.
                    let _ = target.set_pointer_capture(event.pointer_id());
                }

                if let Some(point) = at(&event) {
                    current.set(Some(Stroke {
                        kind: "path".to_owned(),
                        color: color.get_untracked(),
                        width: thickness.get_untracked(),
                        points: vec![point],
                    }));
                }
            }
            on:pointermove=move |event: PointerEvent| {
                let (Some(mut stroke), Some(point)) = (current.get_untracked(), at(&event)) else {
                    return;
                };

                stroke.points.push(point);
                current.set(Some(stroke));
            }
            on:pointerup=move |_| {
                let Some(stroke) = current.get_untracked() else {
                    return;
                };

                let mut next = mine.get_untracked();

                next.push(stroke);
                current.set(None);
                on_change.run(next);
            }
        >
            {move || drawn
                .get()
                .into_iter()
                .map(|stroke| view! {
                    <path
                        d=path_of(&stroke, width.get(), height.get())
                        stroke=stroke.color.clone()
                        stroke-width=stroke.width
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        fill="none"
                    />
                })
                .collect_view()}
        </svg>

        <Show when=move || drawing.get()>
            <div class="absolute left-2 top-2 flex items-center gap-2 rounded-md bg-surface/90 p-1 shadow">
                {COLORS
                    .into_iter()
                    .map(|option| view! {
                        <button
                            class=move || if color.get() == option {
                                "h-5 w-5 rounded-full ring-2 ring-offset-1"
                            } else {
                                "h-5 w-5 rounded-full"
                            }
                            style=format!("background-color: {option}")
                            aria-label=format!("Draw in {option}")
                            on:click=move |_| color.set(option.to_owned())
                        />
                    })
                    .collect_view()}

                <input
                    type="range"
                    min="1"
                    max="10"
                    class="w-20"
                    prop:value=move || thickness.get()
                    on:input=move |event| {
                        thickness.set(event_target_value(&event).parse().unwrap_or(3.0));
                    }
                />

                <button
                    class="text-xs text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                    data-testid="undo-stroke"
                    prop:disabled=move || mine.get().is_empty()
                    on:click=move |_| {
                        let mut next = mine.get_untracked();

                        next.pop();
                        on_change.run(next);
                    }
                >
                    "undo"
                </button>

                <button
                    class="text-xs text-ink-3 hover:text-ink underline-offset-2 hover:underline"
                    prop:disabled=move || mine.get().is_empty()
                    on:click=move |_| on_change.run(Vec::new())
                >
                    "clear mine"
                </button>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stroke_is_drawn_where_the_page_says_not_where_the_screen_does() {
        let stroke = Stroke {
            kind: "path".to_owned(),
            color: "#000".to_owned(),
            width: 2.0,
            points: vec![(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)],
        };

        // The same mark, on two very different screens.
        assert_eq!(path_of(&stroke, 400.0, 800.0), "M 0 0 L 200 200 L 400 800");
        assert_eq!(
            path_of(&stroke, 1200.0, 2400.0),
            "M 0 0 L 600 600 L 1200 2400"
        );
    }

    #[test]
    fn a_single_point_is_still_a_path() {
        let dot = Stroke {
            kind: "path".to_owned(),
            color: "#000".to_owned(),
            width: 2.0,
            points: vec![(0.25, 0.5)],
        };

        assert_eq!(path_of(&dot, 200.0, 100.0), "M 50 50");
    }
}
