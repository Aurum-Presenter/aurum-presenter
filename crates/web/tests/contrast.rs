//! The palette, measured rather than eyeballed.
//!
//! Every colour in the client comes from a token in `web/app.css`, so the whole question "is this
//! readable" can be settled once, here, instead of per screen. The ink ramp stops where it does
//! because of this test: `--ink-4` is the last step that clears 4.5:1 on the lightest surface it
//! lands on, and the test is what stops a later eye from darkening it by one notch.
//!
//! Acceptance criterion 2 of the stage-dark change request.

const STYLESHEET: &str = include_str!("../../../web/app.css");

/// Text roles, and the surfaces each of them is allowed to sit on.
const INK: [&str; 4] = ["ink", "ink-2", "ink-3", "ink-4"];
const SIGNALS: [&str; 5] = ["accent", "live-ink", "ok", "warn", "ink-3"];
const SURFACES: [&str; 3] = ["ground", "surface", "raised"];

/// WCAG AA for body text. Everything in this app is body text or smaller.
const AA: f64 = 4.5;

fn channel(value: f64) -> f64 {
    let value = value / 255.0;

    if value <= 0.03928 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    let part =
        |at: usize| channel(u8::from_str_radix(&hex[at..at + 2], 16).expect("a hex pair") as f64);

    0.2126 * part(0) + 0.7152 * part(2) + 0.0722 * part(4)
}

fn contrast(left: &str, right: &str) -> f64 {
    let (a, b) = (luminance(left), luminance(right));
    let (high, low) = if a > b { (a, b) } else { (b, a) };

    (high + 0.05) / (low + 0.05)
}

/// The custom properties of one block, by name.
fn tokens(block: &str) -> Vec<(String, String)> {
    block
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("--")?;
            let (name, value) = rest.split_once(':')?;
            let value = value.trim().trim_end_matches(';').trim();

            value
                .starts_with('#')
                .then(|| (name.trim().to_owned(), value.to_owned()))
        })
        .collect()
}

/// The two theme sets: the bare `:root` block, and the one inside the light media query.
fn theme(light: bool) -> Vec<(String, String)> {
    let from = if light {
        STYLESHEET
            .find("@media (prefers-color-scheme: light)")
            .expect("a light theme")
    } else {
        0
    };

    let start = STYLESHEET[from..].find(":root").expect("a :root block") + from;
    let open = STYLESHEET[start..].find('{').expect("an opening brace") + start;
    let close = STYLESHEET[open..].find('}').expect("a closing brace") + open;

    tokens(&STYLESHEET[open + 1..close])
}

fn find<'a>(held: &'a [(String, String)], name: &str) -> &'a str {
    held.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("the stylesheet defines --{name}"))
}

fn check(light: bool) {
    let held = theme(light);
    let named = if light { "light" } else { "dark" };
    let mut failures = Vec::new();

    for role in INK.iter().chain(SIGNALS.iter()) {
        for surface in SURFACES {
            let measured = contrast(find(&held, role), find(&held, surface));

            if measured < AA {
                failures.push(format!(
                    "  {named}: --{role} ({}) on --{surface} ({}) is {measured:.2}:1",
                    find(&held, role),
                    find(&held, surface),
                ));
            }
        }
    }

    // The accent button is the one fill that carries text of its own.
    let on_accent = contrast(find(&held, "on-accent"), find(&held, "accent"));

    if on_accent < AA {
        failures.push(format!(
            "  {named}: --on-accent on --accent is {on_accent:.2}:1"
        ));
    }

    assert!(
        failures.is_empty(),
        "text below {AA}:1 on a surface it is used on:\n{}",
        failures.join("\n"),
    );
}

#[test]
fn every_text_token_is_readable_on_every_surface_it_lands_on() {
    check(false);
    check(true);
}

/// White on the live fill: the LIVE badge, and the one place the app puts text on a signal colour.
#[test]
fn the_live_badge_carries_its_own_text() {
    for light in [false, true] {
        let held = theme(light);
        let measured = contrast("#ffffff", find(&held, "live"));

        assert!(
            measured >= AA,
            "white on --live ({}) is {measured:.2}:1",
            find(&held, "live"),
        );
    }
}

/// A screen in a room is black whatever the operating system prefers, so its text cannot come
/// from a token that follows the theme.
#[test]
fn the_output_surfaces_do_not_follow_the_theme() {
    let source = include_str!("../src/present/stage.rs");

    assert!(
        source.contains("bg-black text-white"),
        "the stage view paints itself black with fixed light text",
    );
    assert!(
        !source.contains("bg-black text-ink"),
        "a themed ink token on the stage view would vanish in the light theme",
    );
}
