//! Fractional ranks for set items (business rule 1).
//!
//! An integer position means that inserting one item renumbers every item after it, and two
//! people reordering offline produce two conflicting renumberings of the same rows. A rank is a
//! string between its neighbours instead: inserting touches exactly one row, and a merge is
//! decided by string order rather than by who pushed last.
//!
//! The one invariant that makes this total: a rank never ends in the lowest digit, so there is
//! always room to insert before it.

use thiserror::Error;

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RankError {
    #[error("ranks out of order: {before} is not before {after}")]
    OutOfOrder { before: String, after: String },
    #[error("no rank fits between {before} and {after}")]
    NoRoom { before: String, after: String },
    #[error("'{0}' is not a rank digit")]
    NotADigit(char),
}

/// `None` on either side means "before everything" or "after everything".
pub fn rank_between(before: Option<&str>, after: Option<&str>) -> Result<String, RankError> {
    let low = before.unwrap_or("");
    let high = after.unwrap_or("");

    if !low.is_empty() && !high.is_empty() && low >= high {
        return Err(RankError::OutOfOrder {
            before: low.to_owned(),
            after: high.to_owned(),
        });
    }

    let mut rank = String::new();

    // While the rank still matches `high` digit for digit, the upper bound is `high`'s next
    // digit. The moment it drops below one, every later digit is free — "0z" is below "1"
    // whatever follows it.
    let mut bounded = !high.is_empty();

    for index in 0.. {
        let from = match low.as_bytes().get(index) {
            Some(byte) => digit(*byte)?,
            None => 0,
        };

        let to = if bounded {
            match high.as_bytes().get(index) {
                Some(byte) => digit(*byte)?,
                None => {
                    return Err(RankError::NoRoom {
                        before: low.to_owned(),
                        after: high.to_owned(),
                    });
                }
            }
        } else {
            DIGITS.len()
        };

        if from + 1 < to {
            // A midpoint exists here. It is never the lowest digit, which is what keeps the next
            // insertion before it possible.
            rank.push(DIGITS[(from + to) / 2] as char);
            return Ok(rank);
        }

        rank.push(DIGITS[from] as char);
        bounded = bounded && from == to;
    }

    unreachable!("the loop returns or grows the rank")
}

/// Ranks for a fresh list, evenly spread so the first few inserts stay short.
pub fn initial_ranks(count: usize) -> Result<Vec<String>, RankError> {
    let mut ranks: Vec<String> = Vec::with_capacity(count);

    for _ in 0..count {
        ranks.push(rank_between(ranks.last().map(String::as_str), None)?);
    }

    Ok(ranks)
}

/// The rank an item needs to land at `to` in the current order, having been removed from
/// wherever it was. `None` when the move is a no-op.
pub fn rank_for_move(
    ranks: &[String],
    from: usize,
    to: usize,
) -> Result<Option<String>, RankError> {
    if from == to {
        return Ok(None);
    }

    let without: Vec<&String> = ranks
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != from)
        .map(|(_, rank)| rank)
        .collect();

    let before = if to == 0 {
        None
    } else {
        without.get(to - 1).map(|rank| rank.as_str())
    };

    rank_between(before, without.get(to).map(|rank| rank.as_str())).map(Some)
}

fn digit(byte: u8) -> Result<usize, RankError> {
    DIGITS
        .iter()
        .position(|digit| *digit == byte)
        .ok_or(RankError::NotADigit(byte as char))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn between(before: Option<&str>, after: Option<&str>) -> String {
        rank_between(before, after).expect("a rank")
    }

    #[test]
    fn produces_a_rank_between_its_neighbours() {
        let a = between(None, None);
        let b = between(Some(&a), None);
        let middle = between(Some(&a), Some(&b));

        assert!(a < middle);
        assert!(middle < b);
    }

    #[test]
    fn never_ends_in_the_lowest_digit_so_there_is_always_room_to_insert_before_it() {
        let mut rank = between(None, None);

        for _ in 0..200 {
            rank = between(None, Some(&rank));

            assert!(!rank.ends_with('0'));
            assert!(rank.len() < 210);
        }
    }

    #[test]
    fn keeps_splitting_the_same_gap_without_collapsing() {
        let mut low = between(None, None);
        let high = between(Some(&low), None);

        for _ in 0..200 {
            let next = between(Some(&low), Some(&high));

            assert!(low < next && next < high);
            low = next;
        }
    }

    #[test]
    fn refuses_neighbours_given_in_the_wrong_order() {
        assert_eq!(
            rank_between(Some("b"), Some("a")),
            Err(RankError::OutOfOrder {
                before: "b".to_owned(),
                after: "a".to_owned()
            })
        );
    }

    #[test]
    fn refuses_a_rank_that_is_not_made_of_rank_digits() {
        assert_eq!(
            rank_between(Some("!"), None),
            Err(RankError::NotADigit('!'))
        );
    }

    /// Acceptance criterion 1: dragging the last item to the top, on two devices, merges by
    /// string order rather than by a renumbering that has to win.
    #[test]
    fn moves_an_item_to_the_top_without_touching_the_others() {
        let ranks = initial_ranks(5).unwrap();
        let moved = rank_for_move(&ranks, 4, 0).unwrap().unwrap();

        assert!(moved < ranks[0]);
        assert_eq!(rank_for_move(&ranks, 2, 2), Ok(None));
    }

    #[test]
    fn moves_an_item_into_the_middle() {
        let ranks = initial_ranks(5).unwrap();
        let moved = rank_for_move(&ranks, 0, 3).unwrap().unwrap();

        let mut order: Vec<(String, usize)> = ranks
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, rank)| (rank.clone(), index))
            .collect();
        order.push((moved, 0));
        order.sort();

        assert_eq!(
            order.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
            [1, 2, 3, 0, 4]
        );
    }

    /// Ranks sort as plain strings — that is the whole point of the scheme.
    #[test]
    fn sorts_by_plain_string_order() {
        let mut ranks = ["b".to_owned(), "a".to_owned(), "aV".to_owned()];
        ranks.sort();

        assert_eq!(ranks, ["a", "aV", "b"]);
    }
}
