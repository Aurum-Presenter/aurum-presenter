//! The console commands, as subcommands of the one binary.
//!
//! `laminas-cli` and its four command classes become a `clap` enum. Nothing about what they do
//! changes; there is simply one executable now, and `aurum-api serve` is the default.

use aurum_core::storage::keys;
use clap::{Parser, Subcommand};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message as Email, Tokio1Executor};

use crate::config::Config;
use crate::db::{blocking, now_ms};
use crate::error::{ApiError, ApiResult};
use crate::repo::{in_seconds, invites, sessions};
use crate::state::AppState;

/// Tables that carry tombstones, and are therefore worth sweeping.
const TOMBSTONED_TABLES: [&str; 8] = [
    "folders",
    "songs",
    "arrangements",
    "sheets",
    "annotations",
    "sets",
    "set_items",
    "preferences",
];

const OP_GRACE_SECONDS: i64 = 86_400;
const TOMBSTONE_GRACE_SECONDS: i64 = 30 * 86_400;

#[derive(Parser)]
#[command(
    name = "aurum-api",
    about = "The Aurum Presenter server and its console commands."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Serve the API and the signalling relay.
    Serve,
    /// Apply pending migrations to the control database and every workspace file.
    Migrate,
    /// Remove applied op ids, expired sessions, old tombstones and unreferenced files.
    #[command(name = "maintenance:purge")]
    Purge,
    /// List the workspace databases on this host.
    #[command(name = "workspace:list")]
    ListWorkspaces,
    /// Send whatever is waiting in the mail queue.
    #[command(name = "mail:send")]
    SendMail {
        #[arg(long, default_value_t = 50)]
        limit: i64,
    },
}

pub async fn run(command: Command, config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState::build(config).await?;

    match command {
        Command::Serve => unreachable!("handled in main"),
        Command::Migrate => migrate(&state)?,
        Command::Purge => purge(&state).await?,
        Command::ListWorkspaces => list_workspaces(&state),
        Command::SendMail { limit } => send_mail(&state, limit).await?,
    }

    Ok(())
}

fn migrate(state: &AppState) -> ApiResult<()> {
    let applied = state.db.migrate_control()?;

    println!("control.sqlite: {}", describe(&applied));

    for id in state.db.all_workspaces() {
        println!("{id}: {}", describe(&state.db.migrate_workspace(&id)?));
    }

    Ok(())
}

fn describe(applied: &[String]) -> String {
    match applied {
        [] => "up to date".to_owned(),
        versions => format!("applied {}", versions.join(", ")),
    }
}

fn list_workspaces(state: &AppState) {
    let ids = state.db.all_workspaces();

    if ids.is_empty() {
        println!("No workspace databases yet.");

        return;
    }

    for id in ids {
        let path = state.db.workspace_path(&id);
        let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);

        println!("{id}  {:>8} KB  {}", size / 1024, path.display());
    }
}

async fn purge(state: &AppState) -> ApiResult<()> {
    let expired = {
        let state = state.clone();

        blocking(move || sessions::purge_expired(&state.db.open_control()?)).await?
    };

    println!("Expired sessions removed: {expired}");

    let op_cutoff = in_seconds(-OP_GRACE_SECONDS);
    let tombstone_cutoff = in_seconds(-TOMBSTONE_GRACE_SECONDS);
    let (mut ops, mut tombstones, mut conflicts, mut objects) = (0, 0, 0, 0);

    for id in state.db.all_workspaces() {
        let referenced = {
            let state = state.clone();
            let (id, op_cutoff, tombstone_cutoff) =
                (id.clone(), op_cutoff.clone(), tombstone_cutoff.clone());

            blocking(move || {
                let db = state.db.open_workspace(&id)?;
                let mut removed = (0, 0, 0);

                removed.0 = db.execute(
                    "DELETE FROM applied_ops WHERE applied_at < ?1",
                    [&op_cutoff],
                )?;
                removed.1 = db.execute(
                    "DELETE FROM sync_conflicts WHERE at < ?1",
                    [&tombstone_cutoff],
                )?;

                for table in TOMBSTONED_TABLES {
                    removed.2 += db.execute(
                        &format!(
                            "DELETE FROM {table} WHERE deleted_at IS NOT NULL AND deleted_at < ?1"
                        ),
                        [&tombstone_cutoff],
                    )?;
                }

                // After the rows have gone, not before: what is still referenced is what is left.
                let mut statement = db.prepare(
                    "SELECT DISTINCT sha256 FROM sheets WHERE sha256 IS NOT NULL
                     UNION
                     SELECT DISTINCT background_value FROM presenter_themes
                      WHERE background_kind = 'image'",
                )?;
                let referenced: Vec<String> = statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|hash| hash.to_lowercase())
                    .collect();

                Ok((removed, referenced))
            })
            .await?
        };

        let ((removed_ops, removed_conflicts, removed_tombstones), referenced) = referenced;

        ops += removed_ops;
        conflicts += removed_conflicts;
        tombstones += removed_tombstones;

        let written_before = now_ms() - TOMBSTONE_GRACE_SECONDS * 1000;
        let borrowed: Vec<&str> = referenced.iter().map(String::as_str).collect();

        for prefix in [keys::sheet_prefix(&id), keys::asset_prefix(&id)] {
            let stored = state.storage.list_prefix(&prefix).await?;
            let candidates: Vec<keys::StoredObject<'_>> = stored
                .iter()
                .map(|object| keys::StoredObject {
                    key: &object.key,
                    modified_ms: object.modified_ms,
                })
                .collect();

            for key in keys::orphans(&candidates, &borrowed, written_before) {
                state.storage.delete(key).await?;
                objects += 1;
            }
        }
    }

    println!(
        "Purged {ops} applied ops, {tombstones} tombstones, {conflicts} conflict records, \
         {objects} stored files."
    );

    Ok(())
}

async fn send_mail(state: &AppState, limit: i64) -> ApiResult<()> {
    let queued = {
        let state = state.clone();

        blocking(move || invites::unsent_mail(&state.db.open_control()?, limit)).await?
    };

    if queued.is_empty() {
        println!("Nothing waiting.");

        return Ok(());
    }

    let transport = transport(&state.config.mail.dsn)?;
    let (mut sent, mut failed) = (0, 0);

    for mail in queued {
        let built = Email::builder()
            .from(
                state
                    .config
                    .mail
                    .from
                    .parse()
                    .map_err(|error| ApiError::internal("the configured MAIL_FROM", error))?,
            )
            .to(mail
                .recipient
                .parse()
                .map_err(|error| ApiError::internal("a queued recipient", error))?)
            .subject(&mail.subject)
            .multipart(lettre::message::MultiPart::alternative_plain_html(
                mail.body_text.clone(),
                mail.body_html.clone(),
            ))
            .map_err(|error| ApiError::internal("building an email", error))?;

        let outcome = match &transport {
            // `null://null` swallows mail, which is what a development machine wants.
            None => Ok(()),
            Some(transport) => transport
                .send(built)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
        };

        let state = state.clone();
        let id = mail.id.clone();
        let attempts = mail.attempts;

        match outcome {
            Ok(()) => {
                blocking(move || invites::mark_mail_sent(&state.db.open_control()?, &id)).await?;
                sent += 1;
            }
            Err(error) => {
                blocking(move || {
                    invites::mark_mail_failed(&state.db.open_control()?, &id, attempts, &error)
                })
                .await?;
                failed += 1;
            }
        }
    }

    println!("Sent {sent}, failed {failed}.");

    Ok(())
}

/// `None` for the null transport, so a development machine can queue mail and never send it.
///
/// `smtp://host:port` for a plain connection and `smtps://host:port` for a wrapped one — the two
/// shapes the deployment actually uses. A DSN this cannot read is a startup-shaped mistake, and
/// says so rather than quietly swallowing the mail.
fn transport(dsn: &str) -> ApiResult<Option<AsyncSmtpTransport<Tokio1Executor>>> {
    if dsn.starts_with("null://") {
        return Ok(None);
    }

    let (scheme, rest) = dsn
        .split_once("://")
        .ok_or_else(|| ApiError::internal("MAIL_DSN", "expected scheme://host[:port]"))?;
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()),
        None => (rest, None),
    };

    let builder = match scheme {
        "smtp" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
        "smtps" => AsyncSmtpTransport::<Tokio1Executor>::relay(host)
            .map_err(|error| ApiError::internal("MAIL_DSN", error))?,
        other => {
            return Err(ApiError::internal(
                "MAIL_DSN",
                format!("unknown scheme \"{other}\""),
            ));
        }
    };

    Ok(Some(match port {
        Some(port) => builder.port(port).build(),
        None => builder.build(),
    }))
}
