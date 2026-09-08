//! The client.
//!
//! Client-rendered, deliberately: the app has to start from a precached shell with the radio off,
//! which rules out server-side rendering and hydration.
use aurum_core::chart::chordpro::Chart;
use aurum_core::chart::notes::Key;
use aurum_core::chart::render::{Layout, RenderOptions};
use aurum_core::sync::schema;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

/// Proves the claim the whole migration rests on, in the one place it can be seen: this page
/// transposes a chart with the same code the server links, compiled to WebAssembly. The
/// enharmonic below is spelled by the target key's signature, not by a lookup table — the fourth
/// of Db is Gb here and F# in B, and neither half of the app has its own opinion about it.
#[component]
fn App() -> impl IntoView {
    let key = |text: &str| Key::parse(text).expect("a key");
    let chart = Chart::parse("[F]Amazing [G]grace, how [C]sweet the [F]sound");

    let spelled = move |target: &str| {
        chart
            .render(&RenderOptions {
                source: key("C"),
                target: key(target),
                capo: 0,
                layout: Layout::Inline,
            })
            .sections
            .iter()
            .flat_map(|section| &section.lines)
            .flat_map(|line| &line.segments)
            .filter_map(|segment| segment.chord.clone())
            .collect::<Vec<_>>()
            .join(" ")
    };

    view! {
        <main class="p-6">
            <h1 class="text-xl font-semibold">"Aurum Presenter"</h1>
            <p data-testid="core-version">"Rules from aurum-core " {aurum_core::version()}</p>
            <p data-testid="in-db">"In Db: " {spelled("Db")}</p>
            <p data-testid="in-b">"In B: " {spelled("B")}</p>
            <p data-testid="synced-tables">{schema::tables().join(", ")}</p>
        </main>
    }
}
