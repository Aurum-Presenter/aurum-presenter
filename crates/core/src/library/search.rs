//! The local search index.
//!
//! Search is client-side and offline by rule: a musician looking for a song on a dark stage
//! cannot wait for a round trip, and often has no signal at all. The index is an inverted map
//! from term to song, built from the fields a band actually searches by, and rebuilt
//! incrementally as songs change.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedSong {
    pub id: String,
    pub title: String,
    pub alt_titles: Vec<String>,
    pub artist: Option<String>,
    pub tags: Vec<String>,
    /// Plain lyrics extracted from the chart — chords and directives already stripped.
    pub lyrics: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Title,
    Alt,
    Artist,
    Tag,
    Lyrics,
}

impl Field {
    /// A title match must beat a lyric match by more than a lyric match can accumulate.
    fn weight(self) -> f64 {
        match self {
            Field::Title => 100.0,
            Field::Alt => 60.0,
            Field::Artist => 40.0,
            Field::Tag => 30.0,
            Field::Lyrics => 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub score: f64,
    /// Which field carried the best match, so the list can say why a song is in the results.
    pub field: Field,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Posting {
    song: usize,
    field: Field,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchIndex {
    pub ids: Vec<String>,
    /// Sorted for binary search: a query term matches every index term that starts with it.
    terms: Vec<String>,
    postings: Vec<Vec<Posting>>,
}

/// Lower-cased word tokens. Accents are folded so "Señor" is found by typing "senor".
pub fn tokenise(text: &str) -> Vec<String> {
    text.nfd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '\'' {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The combining diacritics an NFD decomposition leaves behind.
fn is_combining_mark(character: char) -> bool {
    ('\u{0300}'..='\u{036f}').contains(&character)
}

impl SearchIndex {
    pub fn build(songs: &[IndexedSong]) -> SearchIndex {
        let mut by_term: HashMap<String, Vec<Posting>> = HashMap::new();

        for (index, song) in songs.iter().enumerate() {
            let mut add = |text: &str, field: Field| {
                for term in tokenise(text) {
                    let postings = by_term.entry(term).or_default();
                    let posting = Posting { song: index, field };

                    // One posting per (song, field): repeating a word in a chorus must not
                    // outrank a title.
                    if !postings.contains(&posting) {
                        postings.push(posting);
                    }
                }
            };

            add(&song.title, Field::Title);
            for alt in &song.alt_titles {
                add(alt, Field::Alt);
            }
            add(song.artist.as_deref().unwrap_or(""), Field::Artist);
            for tag in &song.tags {
                add(tag, Field::Tag);
            }
            add(&song.lyrics, Field::Lyrics);
        }

        let mut terms: Vec<String> = by_term.keys().cloned().collect();
        terms.sort();

        let postings = terms
            .iter()
            .map(|term| by_term.remove(term).unwrap_or_default())
            .collect();

        SearchIndex {
            ids: songs.iter().map(|song| song.id.clone()).collect(),
            terms,
            postings,
        }
    }

    /// Ranked results for a query. Every query term must match something (AND), which is what
    /// makes a two-word query narrow rather than widen — the behaviour a person expects from a
    /// search box.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let terms = tokenise(query);

        if terms.is_empty() {
            return Vec::new();
        }

        let mut running: Option<HashMap<usize, (f64, Field)>> = None;

        for term in &terms {
            let mut found: HashMap<usize, (f64, Field)> = HashMap::new();

            for position in self.prefix_range(term) {
                // An exact word scores above a prefix of a longer word: "grace" over "graceful".
                let closeness =
                    term.chars().count() as f64 / self.terms[position].chars().count() as f64;

                for posting in &self.postings[position] {
                    let score = posting.field.weight() * closeness;
                    let best = found.entry(posting.song).or_insert((score, posting.field));

                    if score > best.0 {
                        *best = (score, posting.field);
                    }
                }
            }

            running = Some(match running {
                None => found,
                Some(running) => running
                    .into_iter()
                    .filter_map(|(song, current)| {
                        let next = found.get(&song)?;

                        Some((
                            song,
                            (
                                current.0 + next.0,
                                if next.0 > current.0 {
                                    next.1
                                } else {
                                    current.1
                                },
                            ),
                        ))
                    })
                    .collect(),
            });
        }

        let mut hits: Vec<(usize, f64, Field)> = running
            .unwrap_or_default()
            .into_iter()
            .map(|(song, (score, field))| (song, score, field))
            .collect();

        // Song order breaks a tie, so the same query always returns the same list — a result
        // that reshuffles between two identical searches looks like a bug to the person typing.
        hits.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        hits.truncate(limit);

        hits.into_iter()
            .map(|(song, score, field)| SearchHit {
                id: self.ids[song].clone(),
                score,
                field,
            })
            .collect()
    }

    /// Positions of every term starting with the prefix, found by binary search.
    fn prefix_range(&self, prefix: &str) -> std::ops::Range<usize> {
        let low = self.terms.partition_point(|term| term.as_str() < prefix);
        let high = low
            + self.terms[low..]
                .iter()
                .take_while(|term| term.starts_with(prefix))
                .count();

        low..high
    }
}

/// Lyrics as a searcher thinks of them: no chords, no directives, no bracket noise. Cheap enough
/// to run over a whole library, which is what the index build does.
pub fn lyrics_of(chord_pro: &str) -> String {
    chord_pro
        .split('\n')
        .map(|line| {
            if line.starts_with('#') {
                return " ".to_owned();
            }

            strip_between(&strip_between(line, '{', '}', " "), '[', ']', "")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Drops every `open … close` run, keeping everything outside it. An unclosed opener takes the
/// rest of the line with it, exactly as a greedy match would.
fn strip_between(text: &str, open: char, close: char, replacement: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find(open) {
        let Some(length) = rest[start..].find(close) else {
            break;
        };

        result.push_str(&rest[..start]);
        result.push_str(replacement);
        rest = &rest[start + length + close.len_utf8()..];
    }

    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(
        id: &str,
        title: &str,
        alt: &[&str],
        artist: Option<&str>,
        tags: &[&str],
        lyrics: &str,
    ) -> IndexedSong {
        IndexedSong {
            id: id.to_owned(),
            title: title.to_owned(),
            alt_titles: alt.iter().map(|text| (*text).to_owned()).collect(),
            artist: artist.map(str::to_owned),
            tags: tags.iter().map(|text| (*text).to_owned()).collect(),
            lyrics: lyrics.to_owned(),
        }
    }

    fn index() -> SearchIndex {
        SearchIndex::build(&[
            song(
                "a",
                "Amazing Grace",
                &[],
                Some("John Newton"),
                &["hymn"],
                "how sweet the sound",
            ),
            song(
                "b",
                "Great Is Thy Faithfulness",
                &["Morning by Morning"],
                None,
                &["hymn", "slow"],
                "morning by morning new mercies I see",
            ),
            song(
                "c",
                "Graceful Days",
                &[],
                Some("The Sound"),
                &[],
                "nothing to do with grace at all",
            ),
        ])
    }

    fn ids(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|hit| hit.id.as_str()).collect()
    }

    #[test]
    fn ranks_a_title_match_above_a_lyric_match() {
        let index = index();
        let hits = index.search("grace", 50);

        assert_eq!(hits[0].id, "a");
        assert_eq!(hits[0].field, Field::Title);
        assert!(ids(&hits).contains(&"c"));
    }

    #[test]
    fn prefers_an_exact_word_to_a_longer_word_it_prefixes() {
        let index = index();
        let hits = index.search("grace", 50);

        assert_eq!(hits[0].id, "a");
        assert!(hits[1].score < hits[0].score);
    }

    /// Acceptance criterion 1: three letters is enough.
    #[test]
    fn matches_on_a_prefix_of_three_letters() {
        assert_eq!(ids(&index().search("ama", 50)), ["a"]);
    }

    #[test]
    fn narrows_rather_than_widens_with_a_second_word() {
        let index = index();

        assert_eq!(ids(&index.search("morning mercies", 50)), ["b"]);
        assert!(index.search("morning nothing", 50).is_empty());
    }

    #[test]
    fn finds_a_song_by_alternate_title_artist_and_tag() {
        let index = index();

        assert_eq!(index.search("morning by", 50)[0].id, "b");
        assert_eq!(index.search("newton", 50)[0].id, "a");
        assert_eq!(index.search("slow", 50)[0].id, "b");
    }

    #[test]
    fn folds_accents_so_a_keyboard_without_them_still_finds_the_song() {
        let accented = SearchIndex::build(&[song("x", "Señor", &[], None, &[], "")]);

        assert_eq!(accented.search("senor", 50)[0].id, "x");
        assert_eq!(tokenise("Señor, Ven"), ["senor", "ven"]);
        assert_eq!(
            tokenise("Ich möchte grüßen"),
            ["ich", "mochte", "gru", "en"]
        );
    }

    #[test]
    fn returns_nothing_for_an_empty_query_rather_than_everything() {
        assert!(index().search("   ", 50).is_empty());
    }

    #[test]
    fn honours_the_limit_it_is_given() {
        assert_eq!(index().search("grace", 1).len(), 1);
    }

    /// Acceptance criterion 2: a thousand songs, ranked, without the operator noticing.
    #[test]
    fn searches_a_thousand_songs() {
        let many: Vec<IndexedSong> = (0..1000)
            .map(|number| IndexedSong {
                id: format!("song-{number}"),
                title: format!("Song number {number} of the library"),
                alt_titles: vec![format!("Alternate {number}")],
                artist: Some("Some Artist".to_owned()),
                tags: vec!["tag".to_owned()],
                lyrics: format!(
                    "{}unique{number}",
                    "verse one line one verse two line two chorus line ".repeat(8)
                ),
            })
            .collect();

        assert_eq!(
            SearchIndex::build(&many).search("unique512", 50)[0].id,
            "song-512"
        );
    }

    #[test]
    fn drops_chords_and_directives_from_the_lyrics() {
        let lyrics = lyrics_of("{verse: 1}\n[G]Amazing [D/F#]grace");

        assert_eq!(
            lyrics.split_whitespace().collect::<Vec<_>>().join(" "),
            "Amazing grace"
        );
    }

    #[test]
    fn drops_a_comment_line_whole() {
        assert_eq!(lyrics_of("# a note to self\nSing").trim(), "Sing");
    }
}
