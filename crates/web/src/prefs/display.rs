//! What a screen should look like, remembered on that screen only.

use serde::{Deserialize, Serialize};

use crate::app::storage;

const KEY: &str = "aurum.display";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Inline,
    #[default]
    Over,
    Nashville,
}

impl Layout {
    pub fn as_str(self) -> &'static str {
        match self {
            Layout::Inline => "inline",
            Layout::Over => "over",
            Layout::Nashville => "nashville",
        }
    }

    pub fn parse(value: &str) -> Layout {
        match value {
            "inline" => Layout::Inline,
            "nashville" => Layout::Nashville,
            _ => Layout::Over,
        }
    }

    pub fn to_core(self) -> aurum_core::chart::render::Layout {
        match self {
            Layout::Inline => aurum_core::chart::render::Layout::Inline,
            Layout::Over => aurum_core::chart::render::Layout::Over,
            Layout::Nashville => aurum_core::chart::render::Layout::Nashville,
        }
    }
}

/// Chords-only and lyrics-only are the two modes a player asks for mid-rehearsal.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Both,
    Chords,
    Lyrics,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Both => "both",
            Mode::Chords => "chords",
            Mode::Lyrics => "lyrics",
        }
    }

    pub fn parse(value: &str) -> Mode {
        match value {
            "chords" => Mode::Chords,
            "lyrics" => Mode::Lyrics,
            _ => Mode::Both,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Display {
    pub layout: Layout,
    pub font_size: i64,
    pub columns: u8,
    pub mode: Mode,
    pub show_sections: bool,
}

impl Default for Display {
    fn default() -> Display {
        Display {
            layout: Layout::Over,
            font_size: 16,
            columns: 1,
            mode: Mode::Both,
            show_sections: true,
        }
    }
}

/// A browser with storage blocked still gets a working chart, just not a remembered one.
pub fn load() -> Display {
    storage::read_json(KEY).unwrap_or_default()
}

pub fn save(display: &Display) {
    storage::write_json(KEY, display);
}
