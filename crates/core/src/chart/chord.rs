//! The chord grammar, and what transposition does to a chord.
//!
//! Reading a chord can fail, and failing is ordinary: a chart is typed by a human and the thing
//! in brackets is sometimes a stage direction. So parsing returns `None` rather than erroring,
//! the caller keeps the text it could not read, and the editor flags it in the gutter.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::notes::{Key, Letter, Note};

/// The chord grammar of business rule 3: root, optional accidental, optional quality, optional
/// bass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chord {
    pub root: Note,
    /// The suffix exactly as written: `m7`, `sus4`, `maj9#11`. Transposition never touches it.
    pub quality: String,
    pub bass: Option<Note>,
}

/// A token in chord position: either a chord, or text we could not read and must not lose.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordToken {
    pub text: String,
    pub chord: Option<Chord>,
    /// `N.C.` and friends: not a chord, but not a mistake either — no gutter warning.
    pub marker: bool,
}

const MARKERS: [&str; 5] = ["n.c.", "nc", "x", "%", "/"];

/// Quality atoms. A quality is valid when it is a concatenation of these — which accepts `m7b5`,
/// `maj9#11`, `sus4add9` and `6/9`, and rejects `mm`, `zz` and `Hmm`.
const ATOMS: [&str; 30] = [
    "maj", "major", "min", "minor", "sus2", "sus4", "sus", "add", "dim", "aug", "alt", "no", "m",
    "M", "Δ", "°", "ø", "+", "-", "(", ")", ",", " ", "/", "#", "b", "♯", "♭", "2", "3",
];

const NUMBERS: [&str; 9] = ["13", "11", "9", "7", "6", "5", "4", "3", "2"];

impl Chord {
    pub fn parse(text: &str) -> Option<Chord> {
        let token = text.trim();

        if token.is_empty() {
            return None;
        }

        let (head, bass) = split_bass(token);
        let (root, rest) = parse_root(head)?;

        if !is_quality(rest) {
            return None;
        }

        Some(Chord {
            root,
            quality: rest.to_owned(),
            bass,
        })
    }

    /// Transposes by an interval and spells the result in the target key. The bass moves by the
    /// same interval and obeys the same spelling rules (business rule 10).
    pub fn transposed(&self, semitones: i16, target: &Key) -> Transposed {
        let root = target.spell(self.root.pitch_class() + semitones);
        let bass = self
            .bass
            .map(|note| target.spell(note.pitch_class() + semitones));

        Transposed {
            chord: Chord {
                root: root.note,
                quality: self.quality.clone(),
                bass: bass.map(|spelled| spelled.note),
            },
            respelled: root.respelled || bass.is_some_and(|spelled| spelled.respelled),
        }
    }

    /// Nashville numbers: `G C D Em` in G becomes `1 4 5 6m`, slash chords becoming `4/6`.
    pub fn nashville(&self, key: &Key) -> String {
        let bass = match &self.bass {
            Some(note) => format!("/{}", key.degree_of(note)),
            None => String::new(),
        };

        format!("{}{}{bass}", key.degree_of(&self.root), self.quality)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.root, self.quality)?;

        match &self.bass {
            Some(bass) => write!(formatter, "/{bass}"),
            None => Ok(()),
        }
    }
}

/// A chord root is written with a capital letter — `be` is a word, `Be` is not a chord either,
/// but `b` alone must never be read as a root or every flat becomes one.
fn parse_root(head: &str) -> Option<(Note, &str)> {
    let letter = Letter::from_char(head.chars().next().filter(|c| c.is_ascii_uppercase())?)?;

    let accidental = ["##", "bb", "#", "b", "♯", "♭"]
        .into_iter()
        .find(|candidate| head[1..].starts_with(candidate))
        .unwrap_or("");

    Some((
        Note {
            letter,
            alter: match accidental {
                "#" | "♯" => 1,
                "##" => 2,
                "b" | "♭" => -1,
                "bb" => -2,
                _ => 0,
            },
        },
        &head[1 + accidental.len()..],
    ))
}

/// Splits `Am7/G` into `Am7` and the bass note. A trailing `6/9` is a quality, not a bass.
fn split_bass(token: &str) -> (&str, Option<Note>) {
    let Some(slash) = token.rfind('/').filter(|position| *position > 0) else {
        return (token, None);
    };

    match Note::parse(&token[slash + 1..]) {
        Some(bass) => (&token[..slash], Some(bass)),
        None => (token, None),
    }
}

fn is_quality(quality: &str) -> bool {
    let mut rest = quality;

    'outer: while !rest.is_empty() {
        for atom in NUMBERS.into_iter().chain(ATOMS) {
            if let Some(remainder) = rest.strip_prefix(atom) {
                rest = remainder;
                continue 'outer;
            }
        }

        return false;
    }

    true
}

impl ChordToken {
    /// Reads a token in chord position, keeping unreadable text rather than discarding it.
    pub fn read(text: &str) -> ChordToken {
        let chord = Chord::parse(text);

        ChordToken {
            marker: chord.is_none() && MARKERS.contains(&text.trim().to_lowercase().as_str()),
            text: text.to_owned(),
            chord,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transposed {
    pub chord: Chord,
    /// True when the key's spelling would have needed a double accidental — business rule 14.
    pub respelled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        Key::parse(text).expect("a key")
    }

    fn chord(text: &str) -> Chord {
        Chord::parse(text).expect("a chord")
    }

    #[test]
    fn reads_root_quality_and_bass() {
        assert_eq!(chord("Am7/G").to_string(), "Am7/G");
        assert_eq!(chord("Bbmaj9#11").to_string(), "Bbmaj9#11");
        assert_eq!(chord("F#sus4").to_string(), "F#sus4");
        assert_eq!(chord("C6/9").to_string(), "C6/9");
        assert_eq!(chord("C6/9").bass, None);
        assert_eq!(chord("Am7/G").bass, Note::parse("G"));
    }

    #[test]
    fn rejects_anything_that_is_not_a_chord() {
        assert_eq!(Chord::parse("Hmm"), None);
        assert_eq!(Chord::parse("Be"), None);
        assert_eq!(Chord::parse("the"), None);
        assert_eq!(Chord::parse(""), None);
    }

    /// Acceptance criterion 3 — the bass moves with the chord.
    #[test]
    fn moves_a_slash_bass_by_the_same_interval() {
        assert_eq!(
            chord("Am7/G").transposed(5, &key("Dm")).chord.to_string(),
            "Dm7/C"
        );
    }

    #[test]
    fn keeps_the_quality_exactly_as_written() {
        let transposed = chord("Bbmaj9#11").transposed(2, &key("C"));

        assert_eq!(transposed.chord.quality, "maj9#11");
        assert_eq!(transposed.chord.to_string(), "Cmaj9#11");
    }

    /// Acceptance criterion 7.
    #[test]
    fn numbers_degrees_relative_to_the_key() {
        assert_eq!(chord("C/E").nashville(&key("G")), "4/6");
        assert_eq!(chord("Bb").nashville(&key("C")), "b7");
        assert_eq!(chord("Em").nashville(&key("G")), "6m");
    }

    /// Acceptance criterion 6 — unreadable text survives, and only a real mistake is flagged.
    #[test]
    fn keeps_text_it_cannot_read_and_tells_a_marker_from_a_mistake() {
        let unreadable = ChordToken::read("Hmm");

        assert_eq!(unreadable.text, "Hmm");
        assert!(unreadable.chord.is_none());
        assert!(!unreadable.marker);

        assert!(ChordToken::read("N.C.").marker);
        assert!(ChordToken::read("%").marker);
        assert!(ChordToken::read("G").chord.is_some());
    }
}
