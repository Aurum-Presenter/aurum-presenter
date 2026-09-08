//! Notes and keys, spelled the way a key signature spells them.
//!
//! The whole point of this module is that a pitch is not a name. Pitch class 6 is `F#` in B major
//! and `Gb` in Db major, and a transposer that keeps a fixed sharp table gets one of the two wrong
//! every time. So a note is a letter plus an alteration, never a semitone index, and naming a
//! pitch always happens *in a key*.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Letters, not semitones — the distinction is the module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Letter {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl Letter {
    const ALL: [Letter; 7] = [
        Letter::C,
        Letter::D,
        Letter::E,
        Letter::F,
        Letter::G,
        Letter::A,
        Letter::B,
    ];

    const NAMES: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];

    /// Where the letter sits in the octave with no accidental.
    const PITCHES: [i16; 7] = [0, 2, 4, 5, 7, 9, 11];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|letter| *letter == self).unwrap()
    }

    pub fn from_index(index: usize) -> Letter {
        Self::ALL[index % 7]
    }

    pub fn natural_pitch(self) -> i16 {
        Self::PITCHES[self.index()]
    }

    pub fn from_char(character: char) -> Option<Letter> {
        let upper = character.to_ascii_uppercase();

        Self::NAMES
            .iter()
            .position(|name| *name == upper)
            .map(Letter::from_index)
    }
}

impl fmt::Display for Letter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", Letter::NAMES[self.index()])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Note {
    pub letter: Letter,
    /// −2 … +2. Negative is flat, positive is sharp.
    pub alter: i8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    pub tonic: Note,
    pub minor: bool,
}

const MAJOR_STEPS: [i16; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Natural minor, so a minor key can number its own degrees without borrowing the relative
/// major's.
const MINOR_STEPS: [i16; 7] = [0, 2, 3, 5, 7, 8, 10];

const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

impl Note {
    pub fn new(letter: Letter, alter: i8) -> Note {
        Note { letter, alter }
    }

    pub fn pitch_class(&self) -> i16 {
        (self.letter.natural_pitch() + i16::from(self.alter)).rem_euclid(12)
    }

    /// Parses a bare note name. `♯`/`♭` are accepted because that is what a paste from the web
    /// contains, and `x` because that is how a double sharp is usually typed.
    pub fn parse(text: &str) -> Option<Note> {
        let trimmed = text.trim();
        let mut characters = trimmed.chars();
        let letter = Letter::from_char(characters.next()?)?;
        let alter = alter_of(characters.as_str())?;

        Some(Note { letter, alter })
    }
}

impl fmt::Display for Note {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let accidental = if self.alter >= 0 {
            "#".repeat(self.alter as usize)
        } else {
            "b".repeat(self.alter.unsigned_abs() as usize)
        };

        write!(formatter, "{}{accidental}", self.letter)
    }
}

fn alter_of(accidental: &str) -> Option<i8> {
    match accidental {
        "" => Some(0),
        "#" | "♯" => Some(1),
        "##" | "x" => Some(2),
        "b" | "♭" => Some(-1),
        "bb" => Some(-2),
        _ => None,
    }
}

impl Key {
    pub fn new(tonic: Note, minor: bool) -> Key {
        Key { tonic, minor }
    }

    /// Accepts `C`, `Am`, `Bb`, `F#m`, `Ebmin`, `C major`, `a minor`.
    pub fn parse(text: &str) -> Option<Key> {
        let trimmed = text.trim();
        let mut characters = trimmed.chars();
        let letter = Letter::from_char(characters.next()?)?;
        let rest = characters.as_str();

        // The accidental is whichever prefix of the rest is one, longest first: `bb` before `b`,
        // so `Bbm` is B flat minor and not B natural with a `bm` mode.
        let (alter, mode) = ["##", "bb", "#", "b", "♯", "♭", ""]
            .into_iter()
            .find(|candidate| rest.starts_with(candidate))
            .map(|candidate| (alter_of(candidate).unwrap(), &rest[candidate.len()..]))?;

        let minor = match mode.trim().to_ascii_lowercase().as_str() {
            "m" | "min" | "minor" => true,
            "" | "maj" | "major" => false,
            _ => return None,
        };

        Some(Key {
            tonic: Note { letter, alter },
            minor,
        })
    }

    /// The seven notes of the key, spelled by its signature.
    ///
    /// A minor key is spelled from its relative major (business rule 9), so `Am` transposed up
    /// three semitones lands on `Cm` spelled with the Eb-major signature rather than with D#.
    pub fn scale(&self) -> [Note; 7] {
        let steps = if self.minor { MINOR_STEPS } else { MAJOR_STEPS };
        let tonic_pitch = self.tonic.pitch_class();

        std::array::from_fn(|degree| {
            let letter = Letter::from_index(self.tonic.letter.index() + degree);
            let wanted = (tonic_pitch + steps[degree]).rem_euclid(12);

            Note {
                letter,
                alter: normalise_alter(wanted - letter.natural_pitch()),
            }
        })
    }

    /// True when the key signature leans sharp. Chromatic notes are then spelled as a raised
    /// lower degree — which is why B major's raised fourth is `E#` and not `F`.
    pub fn is_sharp(&self) -> bool {
        // C major and A minor have no signature at all; sharps are the reading convention there.
        self.scale()
            .iter()
            .map(|note| i16::from(note.alter))
            .sum::<i16>()
            >= 0
    }

    fn signature_size(&self) -> i16 {
        self.scale()
            .iter()
            .map(|note| i16::from(note.alter.abs()))
            .sum()
    }

    /// Moves a key by semitones, keeping the mode and spelling the new tonic from the target
    /// itself.
    pub fn transposed(&self, semitones: i16) -> Key {
        let pitch = (self.tonic.pitch_class() + semitones).rem_euclid(12) as usize;

        // The target key names itself: of the two candidate spellings, take the one whose own
        // signature is the simpler — Db over C#, but F# over Gb.
        let sharp = Key {
            tonic: Note::parse(SHARP_NAMES[pitch]).unwrap(),
            minor: self.minor,
        };
        let flat = Key {
            tonic: Note::parse(FLAT_NAMES[pitch]).unwrap(),
            minor: self.minor,
        };

        if flat.signature_size() < sharp.signature_size() {
            flat
        } else {
            sharp
        }
    }

    /// Semitones from this key up to another, always 0…11.
    pub fn interval_to(&self, other: &Key) -> i16 {
        (other.tonic.pitch_class() - self.tonic.pitch_class()).rem_euclid(12)
    }

    /// Business rule 13: the same target key is reachable up or down, and the smaller move wins.
    /// Returns the signed interval in −5…+6.
    pub fn shortest_interval_to(&self, other: &Key) -> i16 {
        let up = self.interval_to(other);

        if up > 6 { up - 12 } else { up }
    }

    /// Names a pitch class in this key: diatonic spelling first, then the key's own accidental
    /// side.
    pub fn spell(&self, pitch: i16) -> Spelled {
        let target = pitch.rem_euclid(12);
        let scale = self.scale();

        if let Some(diatonic) = scale.iter().find(|note| note.pitch_class() == target) {
            return Spelled {
                note: *diatonic,
                respelled: false,
            };
        }

        let sharp = self.is_sharp();
        let neighbour_pitch = if sharp {
            (target + 11).rem_euclid(12)
        } else {
            (target + 1).rem_euclid(12)
        };

        if let Some(neighbour) = scale
            .iter()
            .find(|note| note.pitch_class() == neighbour_pitch)
        {
            let candidate = Note {
                letter: neighbour.letter,
                alter: neighbour.alter + if sharp { 1 } else { -1 },
            };

            if candidate.alter.abs() <= 1 {
                return Spelled {
                    note: candidate,
                    respelled: false,
                };
            }
        }

        let names = if sharp { SHARP_NAMES } else { FLAT_NAMES };

        Spelled {
            note: Note::parse(names[target as usize]).unwrap(),
            respelled: true,
        }
    }

    /// Scale degree of a note in this key, as Nashville notation writes it: `1`, `4`, `b3`, `#4`.
    pub fn degree_of(&self, note: &Note) -> String {
        let scale = self.scale();
        let index = (note.letter.index() + 7 - self.tonic.letter.index()) % 7;
        let difference = normalise_alter(note.pitch_class() - scale[index].pitch_class());

        let accidental = if difference > 0 {
            "#".repeat(difference as usize)
        } else {
            "b".repeat(difference.unsigned_abs() as usize)
        };

        format!("{accidental}{}", index + 1)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{}",
            self.tonic,
            if self.minor { "m" } else { "" }
        )
    }
}

/// Brings a raw semitone difference into −6…+5, so B→Cb reads as −1 rather than +11.
fn normalise_alter(difference: i16) -> i8 {
    ((difference + 6).rem_euclid(12) - 6) as i8
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spelled {
    pub note: Note,
    /// True when the key's own spelling would have needed a double accidental (`Fx`, `Bbb`) and a
    /// simpler enharmonic was substituted — business rule 14. The chart header says "respelled".
    pub respelled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Key {
        Key::parse(text).expect("a key")
    }

    #[test]
    fn reads_the_key_names_a_musician_writes() {
        assert_eq!(key("C").to_string(), "C");
        assert_eq!(key("Am").to_string(), "Am");
        assert_eq!(key("Bb").to_string(), "Bb");
        assert_eq!(key("F#m").to_string(), "F#m");
        assert_eq!(key("Ebmin").to_string(), "Ebm");
        assert_eq!(key("C major").to_string(), "C");
        assert_eq!(key("a minor").to_string(), "Am");
        assert_eq!(Key::parse("H"), None);
        assert_eq!(Key::parse("Cx"), None);
    }

    #[test]
    fn spells_a_scale_by_its_signature() {
        let spelled = |text: &str| {
            key(text)
                .scale()
                .iter()
                .map(Note::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        };

        assert_eq!(spelled("C"), "C D E F G A B");
        assert_eq!(spelled("Db"), "Db Eb F Gb Ab Bb C");
        assert_eq!(spelled("B"), "B C# D# E F# G# A#");
        // Business rule 9: a minor key spells from its relative major.
        assert_eq!(spelled("Cm"), "C D Eb F G Ab Bb");
    }

    /// Acceptance criterion 2 — the enharmonic follows the target key signature, not a table.
    #[test]
    fn spells_the_same_pitch_differently_in_two_keys() {
        assert_eq!(key("Db").spell(6).note.to_string(), "Gb");
        assert_eq!(key("B").spell(6).note.to_string(), "F#");
    }

    /// Business rule 14 — no Fx, no Bbb; the chart reports that it was respelled.
    #[test]
    fn falls_back_to_a_simpler_enharmonic_rather_than_a_double_accidental() {
        let spelled = key("C#").spell(2);

        assert!(spelled.note.alter.abs() <= 1);
        assert!(spelled.respelled);
    }

    /// Business rule 13 — the same key is reachable both ways; the shorter move wins.
    #[test]
    fn prefers_the_shorter_direction() {
        assert_eq!(key("C").shortest_interval_to(&key("A")), -3);
        assert_eq!(key("C").shortest_interval_to(&key("D")), 2);
        assert_eq!(key("C").interval_to(&key("A")), 9);
    }

    #[test]
    fn transposing_lets_the_target_key_name_itself() {
        assert_eq!(key("Am").transposed(3).to_string(), "Cm");
        assert_eq!(key("C").transposed(1).to_string(), "Db");
        assert_eq!(key("A").transposed(-3).to_string(), "F#");
        assert_eq!(key("C").transposed(-1).to_string(), "B");
    }

    #[test]
    fn numbers_degrees_the_way_nashville_writes_them() {
        let g = key("G");

        assert_eq!(g.degree_of(&Note::parse("G").unwrap()), "1");
        assert_eq!(g.degree_of(&Note::parse("C").unwrap()), "4");
        assert_eq!(g.degree_of(&Note::parse("E").unwrap()), "6");
        assert_eq!(key("C").degree_of(&Note::parse("Bb").unwrap()), "b7");
        assert_eq!(key("C").degree_of(&Note::parse("F#").unwrap()), "#4");
    }

    #[test]
    fn a_note_is_a_letter_and_an_alteration_not_a_semitone() {
        assert_eq!(Note::parse("F#").unwrap().pitch_class(), 6);
        assert_eq!(Note::parse("Gb").unwrap().pitch_class(), 6);
        assert_ne!(Note::parse("F#").unwrap(), Note::parse("Gb").unwrap());
        assert_eq!(Note::parse("Cb").unwrap().pitch_class(), 11);
        assert_eq!(Note::parse("B♯").unwrap().pitch_class(), 0);
        assert_eq!(Note::parse("Fx").unwrap().to_string(), "F##");
        assert_eq!(Note::parse("Hb"), None);
    }
}
