//! One side of the differential harness: the Rust rules, driven from JSON.
//!
//! Reads `[{"rule": "...", "input": {...}}, …]` on stdin and writes one canonical result per
//! case on stdout. Its twins ran the same cases through the PHP and the TypeScript the port came
//! from; both are gone, and what they established is in `differential/README.md`. The canonical
//! shapes below were defined here and mirrored there — deliberately hand-written on both sides
//! rather than derived from either implementation's own types, so that a shared misunderstanding
//! could not hide a divergence.
//!
//! This side is kept because it still runs: a future port has a corpus and an oracle waiting.

use std::io::Read;

use aurum_core::blobs::policy;
use aurum_core::chart::chordpro::{Chart, ChartLine};
use aurum_core::chart::notes::Key;
use aurum_core::chart::over_lyrics;
use aurum_core::chart::render::{Layout, RenderOptions};
use aurum_core::library::{importer, search};
use aurum_core::present::slides::{Snapshot, SnapshotItem, text_of};
use aurum_core::sets::rank;
use aurum_core::sheets::selection::{self, Part, SheetChoice};
use aurum_core::storage::keys::{self, StoredObject};
use aurum_core::sync::{merge, schema};
use aurum_core::time;
use serde_json::{Value, json};

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("cases on stdin");

    let cases: Vec<Value> = serde_json::from_str(&input).expect("a JSON array of cases");
    let results: Vec<Value> = cases
        .iter()
        .map(|case| {
            let rule = case["rule"].as_str().unwrap_or_default();
            let input = &case["input"];

            match rule {
                "chart" => chart(input),
                "transpose" => transpose(input),
                "over_lyrics" => over_lyrics_case(input),
                "slides" => slides(input),
                "selection" => selection_case(input),
                "rank" => rank_case(input),
                "search" => search_case(input),
                "importer" => importer_case(input),
                "keys" => keys_case(input),
                "time" => time_case(input),
                "pins" => pins(input),
                "sync_schema" => sync_schema(input),
                "object_keys" => object_keys(input),
                "merge" => merge_case(input),
                other => json!({ "unknown_rule": other }),
            }
        })
        .collect();

    println!("{}", serde_json::to_string(&results).expect("serialisable"));
}

fn key_of(value: &Value) -> Option<Key> {
    Key::parse(value.as_str()?)
}

fn lines(line: &ChartLine) -> Value {
    json!({
        "comment": line.comment,
        "segments": line
            .segments
            .iter()
            .map(|segment| json!([
                segment.chord.as_ref().map(|token| token.text.clone()),
                segment.lyric
            ]))
            .collect::<Vec<_>>(),
    })
}

fn chart(input: &Value) -> Value {
    let chart = Chart::parse(input["body"].as_str().unwrap_or_default());

    json!({
        "meta": [
            chart.meta.title, chart.meta.subtitle, chart.meta.artist, chart.meta.key,
            chart.meta.tempo, chart.meta.time, chart.meta.capo,
        ],
        "sections": chart
            .sections
            .iter()
            .map(|section| json!({
                "kind": section.kind.as_str(),
                "label": section.label,
                "lines": section.lines.iter().map(lines).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "warnings": chart
            .warnings
            .iter()
            .map(|warning| json!([warning.line, warning.token]))
            .collect::<Vec<_>>(),
        "error": chart.error.map(|error| json!([error.line, error.message])),
    })
}

fn transpose(input: &Value) -> Value {
    let (Some(source), Some(target)) = (key_of(&input["from"]), key_of(&input["to"])) else {
        return json!({ "error": "not a key" });
    };

    let options = RenderOptions {
        source,
        target,
        capo: input["capo"].as_i64().unwrap_or_default() as i16,
        layout: match input["layout"].as_str().unwrap_or("inline") {
            "over" => Layout::Over,
            "nashville" => Layout::Nashville,
            _ => Layout::Inline,
        },
    };
    let rendered = Chart::parse(input["body"].as_str().unwrap_or_default()).render(&options);

    json!({
        "shape_key": rendered.shape_key.to_string(),
        "respelled": rendered.respelled,
        "lines": rendered
            .sections
            .iter()
            .flat_map(|section| &section.lines)
            .map(|line| json!({
                "inline": line.inline_text(),
                "chords": line.over_lyrics_rows().chords,
                "lyrics": line.over_lyrics_rows().lyrics,
                "rendered": line
                    .segments
                    .iter()
                    .map(|segment| segment.chord.clone())
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    })
}

fn over_lyrics_case(input: &Value) -> Value {
    let text = input["text"].as_str().unwrap_or_default();

    json!({
        "notation": match over_lyrics::detect_notation(text) {
            over_lyrics::Notation::ChordPro => "chordpro",
            over_lyrics::Notation::OverLyrics => "over_lyrics",
            over_lyrics::Notation::Ambiguous => "ambiguous",
        },
        "chordpro": over_lyrics::to_chord_pro(text),
        "chord_line": over_lyrics::is_chord_line(text),
    })
}

fn slides(input: &Value) -> Value {
    let items = input["items"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, item)| SnapshotItem {
            item_id: format!("item-{index}"),
            title: item["title"].as_str().unwrap_or("Untitled").to_owned(),
            body: item["body"].as_str().map(str::to_owned),
            written_key: item["written_key"].as_str().map(str::to_owned),
            set_key: item["set_key"].as_str().map(str::to_owned),
            capo: item["capo"].as_i64().unwrap_or_default() as i16,
            item_type: item["item_type"].as_str().map(str::to_owned),
            content: item["content"].as_str().map(str::to_owned),
            sheet_id: item["sheet_id"].as_str().map(str::to_owned),
            sheet_pages: item["sheet_pages"].as_u64().map(|pages| pages as u32),
            ..SnapshotItem::default()
        })
        .collect();

    let snapshot = Snapshot {
        set_name: "Differential".to_owned(),
        items,
        ..Snapshot::default()
    };

    json!(
        snapshot
            .slides(
                input["font_size_vh"].as_f64().unwrap_or(8.0),
                input["safe_area_pct"].as_f64().unwrap_or(5.0),
            )
            .iter()
            .map(|slide| json!({
                "id": slide.id,
                "kind": format!("{:?}", slide.kind).to_lowercase(),
                "label": slide.label,
                "text": slide.text,
                "page": slide.page,
                "set_key": slide.set_key,
                "capo": slide.capo,
                "lines": slide.lines.iter().map(text_of).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()
    )
}

fn sheet_choices(value: &Value) -> Vec<SheetChoice<'_>> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|sheet| SheetChoice {
            id: sheet["id"].as_str().unwrap_or_default(),
            sheet_key: sheet["sheet_key"].as_str(),
            part: sheet["part"].as_str(),
            position: sheet["position"].as_i64().unwrap_or_default(),
            deleted: sheet["deleted"].as_bool().unwrap_or_default(),
        })
        .collect()
}

fn selection_case(input: &Value) -> Value {
    let sheets = sheet_choices(&input["sheets"]);
    let key = key_of(&input["key"]);
    let part = input["part"].as_str().and_then(Part::parse);

    match selection::select_sheet(&sheets, key.as_ref(), part) {
        None => json!(null),
        Some(found) => json!({
            "id": found.sheet.id,
            "fallback": format!("{:?}", found.fallback),
            "explain": found.explain(key.as_ref(), part),
        }),
    }
}

fn rank_case(input: &Value) -> Value {
    let text = |value: &Value| value.as_str().map(str::to_owned);

    let result = match input["op"].as_str().unwrap_or("between") {
        "initial" => rank::initial_ranks(input["count"].as_u64().unwrap_or_default() as usize)
            .map(|ranks| json!(ranks)),
        "move" => {
            let ranks: Vec<String> = input["ranks"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(text)
                .collect();

            rank::rank_for_move(
                &ranks,
                input["from"].as_u64().unwrap_or_default() as usize,
                input["to"].as_u64().unwrap_or_default() as usize,
            )
            .map(|rank| json!(rank))
        }
        _ => rank::rank_between(input["before"].as_str(), input["after"].as_str())
            .map(|rank| json!(rank)),
    };

    result.unwrap_or_else(|_| json!({ "error": true }))
}

fn search_case(input: &Value) -> Value {
    let songs: Vec<search::IndexedSong> = input["songs"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|song| search::IndexedSong {
            id: song["id"].as_str().unwrap_or_default().to_owned(),
            title: song["title"].as_str().unwrap_or_default().to_owned(),
            alt_titles: strings(&song["alt_titles"]),
            artist: song["artist"].as_str().map(str::to_owned),
            tags: strings(&song["tags"]),
            lyrics: song["lyrics"].as_str().unwrap_or_default().to_owned(),
        })
        .collect();

    json!(
        search::SearchIndex::build(&songs)
            .search(
                input["query"].as_str().unwrap_or_default(),
                input["limit"].as_u64().unwrap_or(50) as usize,
            )
            .iter()
            .map(|hit| json!([
                hit.id,
                (hit.score * 1e6).round() / 1e6,
                format!("{:?}", hit.field).to_lowercase()
            ]))
            .collect::<Vec<_>>()
    )
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn importer_case(input: &Value) -> Value {
    let result = importer::import_file(
        input["filename"].as_str().unwrap_or_default(),
        input["text"].as_str().unwrap_or_default(),
    );

    json!({
        "error": result.error,
        "song": result.song.map(|song| json!({
            "title": song.title,
            "artist": song.artist,
            "original_key": song.original_key,
            "tempo": song.tempo,
            "time_signature": song.time_signature,
            "body": song.body,
            "notation": match song.source_notation {
                over_lyrics::Notation::ChordPro => "chordpro",
                _ => "over_lyrics",
            },
            "source_text": song.source_text,
        })),
    })
}

fn keys_case(input: &Value) -> Value {
    let workspace = input["workspace"].as_str().unwrap_or_default();
    let hash = input["sha256"].as_str().unwrap_or_default();

    json!({
        "sheet": keys::sheet_key(workspace, hash),
        "valid": keys::is_valid_sha256(hash),
        "asset": keys::asset_key(workspace, hash, input["content_type"].as_str().unwrap_or(""))
            .ok(),
    })
}

fn time_case(input: &Value) -> Value {
    json!({
        "parsed": time::parse(input["text"].as_str().unwrap_or_default()),
        "formatted": input["epoch_ms"].as_i64().map(time::format),
    })
}

fn pins(input: &Value) -> Value {
    json!({
        "auto_pinned": policy::is_auto_pinned(
            input["pinned"].as_bool().unwrap_or_default(),
            input["scheduled_for"].as_str(),
            time::parse(input["now"].as_str().unwrap_or_default()).unwrap_or_default(),
        ),
    })
}

fn sync_schema(input: &Value) -> Value {
    let table = input["table"].as_str().unwrap_or_default();
    let known = schema::is_synced_table(table);
    let payload = input["payload"].as_object().cloned().unwrap_or_default();

    json!({
        "known": known,
        "columns": schema::columns(table),
        "viewer": known && schema::is_viewer_writable(table),
        "filtered": schema::filter(table, &payload),
        "tables": schema::tables(),
    })
}

fn object_keys(input: &Value) -> Value {
    let workspace = input["workspace"].as_str().unwrap_or_default();
    let hash = input["sha256"].as_str().unwrap_or_default();
    let objects: Vec<StoredObject<'_>> = input["objects"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|object| StoredObject {
            key: object["key"].as_str().unwrap_or_default(),
            modified_ms: object["modified"].as_i64().unwrap_or_default(),
        })
        .collect();
    let referenced: Vec<&str> = input["referenced"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .collect();

    json!({
        "sheet": keys::sheet_key(workspace, hash),
        "valid": keys::is_valid_sha256(hash),
        "asset": keys::asset_key(workspace, hash, input["content_type"].as_str().unwrap_or("")).ok(),
        "orphans": keys::orphans(&objects, &referenced, input["written_before"].as_i64().unwrap_or_default()),
    })
}

/// The observable outcome of one operation, in the shape `differential/php.php` reports it: not
/// the decision either implementation made, but the row a user would afterwards see.
fn merge_case(input: &Value) -> Value {
    let table = input["table"].as_str().unwrap_or("songs");
    let payload = input["payload"].as_object().cloned().unwrap_or_default();
    let empty = serde_json::Map::new();
    let stored = input["existing"]["fields"].as_object().unwrap_or(&empty);

    let existing = input["existing"].as_object().map(|row| merge::Existing {
        updated_at: row["updated_at"]
            .as_str()
            .unwrap_or("2026-09-06T10:00:00.000Z"),
        deleted_at: row["deleted_at"].as_str(),
        updated_by: row["updated_by"].as_str(),
        fields: stored,
    });

    let op = merge::Op {
        table,
        kind: match input["kind"].as_str() {
            Some("delete") => merge::OpKind::Delete,
            _ => merge::OpKind::Upsert,
        },
        payload,
        base_updated_at: input["base_updated_at"].as_str(),
    };

    let plan = merge::plan(&op, existing.as_ref());

    // Only the columns this case touched, matched to what the PHP reports.
    let allowed = schema::columns(table).unwrap_or(&[]);
    let mut touched: Vec<&str> = allowed
        .iter()
        .copied()
        .filter(|column| op.payload.contains_key(*column) || stored.contains_key(*column))
        .collect();
    touched.sort_unstable();

    let (exists, deleted, after) = match &plan.action {
        merge::Action::Insert(payload) => (true, false, payload.clone()),
        merge::Action::Update(payload) => {
            let mut after = stored.clone();
            after.extend(payload.clone());

            (true, false, after)
        }
        merge::Action::Delete => (true, true, stored.clone()),
        merge::Action::Skip(merge::Skipped::Tombstoned) => (true, true, stored.clone()),
        merge::Action::Skip(merge::Skipped::AlreadyGone) => (false, false, serde_json::Map::new()),
    };

    let text = |value: Option<&Value>| match value {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(text)) => Value::String(text.clone()),
        Some(Value::Bool(flag)) => Value::String(if *flag { "1" } else { "" }.to_owned()),
        Some(other) => Value::String(other.to_string()),
    };

    let mut conflicts: Vec<Value> = plan
        .conflicts
        .iter()
        .map(|conflict| json!([conflict.field, text(Some(&conflict.losing_value))]))
        .collect();
    conflicts.sort_by_key(|conflict| conflict[0].as_str().unwrap_or_default().to_owned());

    json!({
        "exists": exists,
        "deleted": deleted,
        "fields": touched
            .iter()
            .map(|column| ((*column).to_owned(), if exists { text(after.get(*column)) } else { Value::Null }))
            .collect::<serde_json::Map<String, Value>>(),
        "conflicts": conflicts,
        "sibling_default": input["song_id"].as_str().map(|song| {
            if plan.demote_defaults_of.as_deref() == Some(song) { "0" } else { "1" }
        }),
    })
}
