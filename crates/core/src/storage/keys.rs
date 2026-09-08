//! Object keys are content-addressed: `sheets/{workspace}/{sha256}.pdf`.
//!
//! Two consequences fall out of that, both of which the sheet-attachments spec wanted anyway.
//! Re-uploading identical bytes writes the same key, so "a replace producing the same hash is a
//! no-op" stops being a rule the code has to remember. And replacing a file writes a *new*
//! object rather than mutating an existing one, so a device still holding the old URL keeps
//! receiving the bytes it cached instead of silently getting different ones.
//!
//! The workspace id stays in the key even though the content database no longer has that
//! column: it comes from the route, and it keeps a workspace's objects deletable as one prefix.

use thiserror::Error;

pub fn sheet_key(workspace_id: &str, sha256: &str) -> String {
    format!("sheets/{workspace_id}/{}.pdf", sha256.to_lowercase())
}

pub fn sheet_prefix(workspace_id: &str) -> String {
    format!("sheets/{workspace_id}/")
}

pub fn is_valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// What an audience screen can display, and nothing a browser would execute.
const ASSET_TYPES: [(&str, &str); 4] = [
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/avif", "avif"),
];

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("a background must be a PNG, JPEG, WebP or AVIF image.")]
pub struct UnsupportedType;

/// Workspace assets — today, the background image behind an audience slide. Content-addressed
/// under `assets/{workspace}/{sha256}.{ext}` for the same reasons sheets are.
pub fn asset_key(
    workspace_id: &str,
    sha256: &str,
    content_type: &str,
) -> Result<String, UnsupportedType> {
    Ok(format!(
        "assets/{workspace_id}/{}.{}",
        sha256.to_lowercase(),
        extension_for(content_type)?
    ))
}

pub fn asset_prefix(workspace_id: &str) -> String {
    format!("assets/{workspace_id}/")
}

pub fn extension_for(content_type: &str) -> Result<&'static str, UnsupportedType> {
    let content_type = content_type.to_lowercase();

    ASSET_TYPES
        .iter()
        .find(|(mime, _)| *mime == content_type)
        .map(|(_, extension)| *extension)
        .ok_or(UnsupportedType)
}

pub fn asset_content_types() -> Vec<&'static str> {
    ASSET_TYPES.iter().map(|(mime, _)| *mime).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredObject<'a> {
    pub key: &'a str,
    /// When the object store last wrote it, in Unix milliseconds.
    pub modified_ms: i64,
}

/// Which stored files nothing points at any more.
///
/// A replace deliberately leaves the old object where it is, so a device still holding its URL
/// keeps receiving the bytes it cached; this is the rule that decides when it finally goes. Two
/// things stop a file being swept: a row still names its hash, or it was written recently — an
/// upload can reach the object store before its row reaches the server, and a file the store has
/// but the database does not know about yet may be the only copy in existence.
pub fn orphans<'a>(
    objects: &[StoredObject<'a>],
    referenced: &[&str],
    written_before_ms: i64,
) -> Vec<&'a str> {
    let keep: Vec<String> = referenced.iter().map(|hash| hash.to_lowercase()).collect();

    objects
        .iter()
        .filter(|object| {
            let hash = hash_of(object.key);

            !keep.contains(&hash) && object.modified_ms < written_before_ms
        })
        .map(|object| object.key)
        .collect()
}

/// The content hash a key carries: the filename without its directory or extension.
fn hash_of(key: &str) -> String {
    let name = key.rsplit('/').next().unwrap_or(key);

    name.rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn puts_a_sheet_where_its_workspace_can_be_deleted_as_one_prefix() {
        let key = sheet_key("ws-1", HASH);

        assert_eq!(key, format!("sheets/ws-1/{HASH}.pdf"));
        assert!(key.starts_with(&sheet_prefix("ws-1")));
    }

    /// The same bytes are the same object, whoever typed the hash and in which case.
    #[test]
    fn the_same_bytes_land_on_the_same_key() {
        assert_eq!(
            sheet_key("ws-1", &HASH.to_uppercase()),
            sheet_key("ws-1", HASH)
        );
    }

    #[test]
    fn recognises_a_content_hash_and_nothing_else() {
        assert!(is_valid_sha256(HASH));
        assert!(is_valid_sha256(&HASH.to_uppercase()));
        assert!(!is_valid_sha256(&HASH[..63]), "too short");
        assert!(!is_valid_sha256(&format!("{HASH}0")), "too long");
        assert!(!is_valid_sha256(&"g".repeat(64)), "not hex");
        assert!(!is_valid_sha256(""));
    }

    #[test]
    fn stores_a_background_under_the_extension_of_what_it_actually_is() {
        assert_eq!(
            asset_key("ws-1", HASH, "image/jpeg").unwrap(),
            format!("assets/ws-1/{HASH}.jpg")
        );
        assert_eq!(
            asset_key("ws-1", HASH, "IMAGE/PNG").unwrap(),
            format!("assets/ws-1/{HASH}.png")
        );
        assert!(
            asset_key("ws-1", HASH, "image/jpeg")
                .unwrap()
                .starts_with(&asset_prefix("ws-1"))
        );
    }

    /// An audience screen shows pictures. It does not run anything.
    #[test]
    fn refuses_a_type_a_browser_would_execute() {
        assert_eq!(extension_for("image/svg+xml"), Err(UnsupportedType));
        assert_eq!(extension_for("text/html"), Err(UnsupportedType));
        assert_eq!(extension_for("application/pdf"), Err(UnsupportedType));
        assert_eq!(asset_content_types().len(), 4);
    }

    #[test]
    fn sweeps_only_what_nothing_names_and_nothing_just_wrote() {
        let old = 1_000_000;
        let recent = 9_000_000;
        let referenced = format!("sheets/ws-1/{HASH}.pdf");
        let objects = [
            StoredObject {
                key: &referenced,
                modified_ms: old,
            },
            StoredObject {
                key: "sheets/ws-1/aa.pdf",
                modified_ms: old,
            },
            StoredObject {
                key: "sheets/ws-1/bb.pdf",
                modified_ms: recent,
            },
        ]
        .to_vec();

        assert_eq!(
            orphans(&objects, &[HASH], 5_000_000),
            ["sheets/ws-1/aa.pdf"],
            "the referenced one stays, and so does the one written a moment ago"
        );
    }

    /// An upload can reach the store before its row reaches the server; sweeping it would delete
    /// the only copy in existence.
    #[test]
    fn never_sweeps_a_file_the_database_may_not_have_heard_about_yet() {
        let objects = [StoredObject {
            key: "sheets/ws-1/fresh.pdf",
            modified_ms: 5_000_000,
        }];

        assert!(orphans(&objects, &[], 5_000_000).is_empty());
    }

    #[test]
    fn matches_a_reference_however_it_was_written() {
        let referenced = format!("sheets/ws-1/{HASH}.pdf");
        let objects = [StoredObject {
            key: &referenced,
            modified_ms: 0,
        }];

        assert!(orphans(&objects, &[&HASH.to_uppercase()], 5_000_000).is_empty());
    }
}
