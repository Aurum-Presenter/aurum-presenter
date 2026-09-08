//! Which of four places supplied the key a musician is looking at.

use serde::{Deserialize, Serialize};

use super::notes::Key;

/// Business rule 7: set override → preferred key → arrangement default → song original key.
/// The first that holds a key wins, and the UI names the level that supplied it — a musician who
/// sees the wrong key needs to know which of four places to go and change it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    Set,
    Preference,
    Arrangement,
    Song,
    #[default]
    None,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            KeySource::Set => "set",
            KeySource::Preference => "preference",
            KeySource::Arrangement => "arrangement",
            KeySource::Song => "song",
            KeySource::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyInputs<'a> {
    pub set_override: Option<&'a str>,
    pub preferred: Option<&'a str>,
    pub arrangement_default: Option<&'a str>,
    pub song_original: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectiveKey {
    pub key: Option<Key>,
    pub source: KeySource,
}

impl<'a> KeyInputs<'a> {
    /// Walks the four levels in order. A level holding something that is not a key is skipped
    /// rather than fatal: a typo in one preference must not hide the song's own key.
    pub fn effective(&self) -> EffectiveKey {
        let levels = [
            (KeySource::Set, self.set_override),
            (KeySource::Preference, self.preferred),
            (KeySource::Arrangement, self.arrangement_default),
            (KeySource::Song, self.song_original),
        ];

        for (source, value) in levels {
            if let Some(key) = value.filter(|text| !text.is_empty()).and_then(Key::parse) {
                return EffectiveKey {
                    key: Some(key),
                    source,
                };
            }
        }

        EffectiveKey::default()
    }

    /// The key the chart is *written* in, which transposition measures from: the arrangement's
    /// own default if it has one, else the song's original key. Never the reader's preference.
    pub fn source_key(&self) -> Option<Key> {
        KeyInputs {
            arrangement_default: self.arrangement_default,
            song_original: self.song_original,
            ..KeyInputs::default()
        }
        .effective()
        .key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(effective: EffectiveKey) -> (String, &'static str) {
        (
            effective.key.map(|key| key.to_string()).unwrap_or_default(),
            effective.source.as_str(),
        )
    }

    /// Business rule 7.
    #[test]
    fn resolves_in_order_and_names_the_level_that_supplied_the_key() {
        let inputs = KeyInputs {
            set_override: Some("A"),
            preferred: Some("C"),
            song_original: Some("G"),
            ..KeyInputs::default()
        };

        assert_eq!(named(inputs.effective()), ("A".to_owned(), "set"));

        let inputs = KeyInputs {
            set_override: None,
            ..inputs
        };
        assert_eq!(named(inputs.effective()), ("C".to_owned(), "preference"));

        let inputs = KeyInputs {
            preferred: None,
            arrangement_default: Some("E"),
            ..inputs
        };
        assert_eq!(named(inputs.effective()), ("E".to_owned(), "arrangement"));

        let inputs = KeyInputs {
            arrangement_default: None,
            ..inputs
        };
        assert_eq!(named(inputs.effective()), ("G".to_owned(), "song"));

        assert_eq!(KeyInputs::default().effective().source, KeySource::None);
        assert_eq!(KeyInputs::default().effective().key, None);
    }

    #[test]
    fn ignores_a_level_that_holds_something_which_is_not_a_key() {
        let inputs = KeyInputs {
            preferred: Some("not a key"),
            song_original: Some("F#m"),
            ..KeyInputs::default()
        };

        assert_eq!(named(inputs.effective()), ("F#m".to_owned(), "song"));
    }

    /// An empty string is a field a musician cleared, not a key.
    #[test]
    fn an_empty_level_is_no_level_at_all() {
        let inputs = KeyInputs {
            set_override: Some(""),
            song_original: Some("D"),
            ..KeyInputs::default()
        };

        assert_eq!(named(inputs.effective()), ("D".to_owned(), "song"));
    }

    /// Transposition measures from the written key, so the reader's preference cannot move it.
    #[test]
    fn the_source_key_ignores_the_readers_preference() {
        let inputs = KeyInputs {
            set_override: Some("A"),
            preferred: Some("C"),
            arrangement_default: Some("E"),
            song_original: Some("G"),
        };

        assert_eq!(
            inputs.source_key().map(|key| key.to_string()),
            Some("E".to_owned())
        );
        assert_eq!(
            KeyInputs {
                arrangement_default: None,
                ..inputs
            }
            .source_key()
            .map(|key| key.to_string()),
            Some("G".to_owned())
        );
    }
}
