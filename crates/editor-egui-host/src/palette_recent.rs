//! Command-palette recent activation persistence.
//!
//! The persisted file is intentionally only a newline-delimited list of
//! [`Command::diagnostic_id`](rge_editor_ui::menus::Command::diagnostic_id)
//! strings, stored most-recent-first. No labels, enablement state, command
//! metadata, favorites, or alternate command model are serialized here.

use std::io;
use std::path::{Path, PathBuf};

use rge_editor_ui::menus::Command;

use crate::handoff::MenuCommandHandoff;
use crate::menu::{record_command_palette_recent_command, COMMAND_PALETTE_RECENT_COMMAND_LIMIT};

const COMMAND_PALETTE_RECENT_FILE_NAME: &str = "command_palette_recent.txt";

/// Deterministic per-user command-palette recent file path.
#[must_use]
pub(crate) fn default_command_palette_recent_path() -> PathBuf {
    default_config_dir().join(COMMAND_PALETTE_RECENT_FILE_NAME)
}

/// Load command-palette recent ids from `path`.
///
/// The returned ids are capped, deduplicated most-recent-first, and contain
/// only valid one-line diagnostic id strings. Missing, unreadable, or corrupt
/// files are surfaced as an [`io::Error`] so host construction can deliberately
/// ignore them.
pub(crate) fn load_command_palette_recent_command_ids(path: &Path) -> io::Result<Vec<String>> {
    let contents = std::fs::read_to_string(path)?;
    parse_command_palette_recent_ids(&contents)
}

/// Load command-palette recent ids, logging and falling back to empty on error.
#[must_use]
pub(crate) fn load_command_palette_recent_command_ids_or_empty(path: &Path) -> Vec<String> {
    match load_command_palette_recent_command_ids(path) {
        Ok(ids) => ids,
        Err(error) => {
            tracing::debug!(
                target: "rge::editor-egui-host::command-palette",
                path = %path.display(),
                error = %error,
                "command-palette recent ids unavailable; starting with an empty recent list"
            );
            Vec::new()
        }
    }
}

/// Save command-palette recent ids to `path`.
///
/// Parent directories are created on demand. The written file contains only the
/// capped, deduplicated diagnostic id list, one id per line.
pub(crate) fn save_command_palette_recent_command_ids(
    path: &Path,
    recent_command_ids: &[String],
) -> io::Result<()> {
    let ids = normalize_command_palette_recent_ids(
        recent_command_ids.iter().map(String::as_str),
        |index| index + 1,
    )?;
    let mut contents = ids.join("\n");
    if !contents.is_empty() {
        contents.push('\n');
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

pub(crate) fn enqueue_command_palette_activation(
    menu_commands: &MenuCommandHandoff,
    recent_command_ids: &mut Vec<String>,
    recent_path: &Path,
    command: Command,
) {
    let command_id = command.diagnostic_id();
    record_command_palette_recent_command(recent_command_ids, command_id.clone());
    if let Err(error) = save_command_palette_recent_command_ids(recent_path, recent_command_ids) {
        tracing::debug!(
            target: "rge::editor-egui-host::command-palette",
            path = %recent_path.display(),
            command_id = %command_id,
            error = %error,
            "command-palette recent ids could not be saved"
        );
    }
    menu_commands.push(command);
}

fn parse_command_palette_recent_ids(contents: &str) -> io::Result<Vec<String>> {
    normalize_command_palette_recent_ids(contents.lines(), |index| index + 1)
}

fn normalize_command_palette_recent_ids<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    line_number: impl Fn(usize) -> usize,
) -> io::Result<Vec<String>> {
    let mut normalized = Vec::new();
    for (index, id) in ids.into_iter().enumerate() {
        if !is_valid_command_palette_recent_id(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid command-palette recent diagnostic id on line {}",
                    line_number(index)
                ),
            ));
        }
        if !normalized.iter().any(|existing| existing == id) {
            normalized.push(id.to_owned());
            if normalized.len() == COMMAND_PALETTE_RECENT_COMMAND_LIMIT {
                break;
            }
        }
    }
    Ok(normalized)
}

fn is_valid_command_palette_recent_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|ch| !ch.is_control() && !ch.is_whitespace())
}

pub(crate) fn default_config_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Some(appdata) = non_empty_env("APPDATA") {
            return appdata.join("rge");
        }
        if let Some(profile) = non_empty_env("USERPROFILE") {
            return profile.join("AppData").join("Roaming").join("rge");
        }
    } else if let Some(xdg) = non_empty_env("XDG_CONFIG_HOME") {
        return xdg.join("rge");
    }

    if let Some(home) = non_empty_env("HOME").or_else(|| non_empty_env("USERPROFILE")) {
        return home.join(".config").join("rge");
    }
    PathBuf::from(".rge_config")
}

fn non_empty_env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
