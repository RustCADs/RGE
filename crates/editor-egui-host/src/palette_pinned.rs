//! Command-palette pinned command persistence.
//!
//! The pinned-command file mirrors command-palette recent storage: a
//! newline-delimited list of [`Command::diagnostic_id`] strings. It stores only
//! stable ids, not labels, enablement state, shortcut metadata, or a second
//! command model.

use std::io;
use std::path::{Path, PathBuf};

use rge_editor_ui::menus::Command;

use crate::menu::COMMAND_PALETTE_PINNED_COMMAND_LIMIT;
use crate::palette_recent::default_config_dir;

const COMMAND_PALETTE_PINNED_FILE_NAME: &str = "command_palette_pinned.txt";

/// Deterministic per-user command-palette pinned-command file path.
#[must_use]
pub(crate) fn default_command_palette_pinned_path() -> PathBuf {
    default_config_dir().join(COMMAND_PALETTE_PINNED_FILE_NAME)
}

/// Load command-palette pinned ids from `path`.
///
/// The returned ids are capped, deduplicated, and contain only valid one-line
/// diagnostic id strings. Missing, unreadable, or corrupt files are surfaced as
/// an [`io::Error`] so host construction can deliberately ignore them.
pub(crate) fn load_command_palette_pinned_command_ids(path: &Path) -> io::Result<Vec<String>> {
    let contents = std::fs::read_to_string(path)?;
    parse_command_palette_pinned_ids(&contents)
}

/// Load command-palette pinned ids, logging and falling back to empty on error.
#[must_use]
pub(crate) fn load_command_palette_pinned_command_ids_or_empty(path: &Path) -> Vec<String> {
    match load_command_palette_pinned_command_ids(path) {
        Ok(ids) => ids,
        Err(error) => {
            tracing::debug!(
                target: "rge::editor-egui-host::command-palette",
                path = %path.display(),
                error = %error,
                "command-palette pinned ids unavailable; starting with an empty pinned list"
            );
            Vec::new()
        }
    }
}

/// Save command-palette pinned ids to `path`.
///
/// Parent directories are created on demand. The written file contains only the
/// capped, deduplicated diagnostic id list, one id per line.
pub(crate) fn save_command_palette_pinned_command_ids(
    path: &Path,
    pinned_command_ids: &[String],
) -> io::Result<()> {
    let ids = normalize_command_palette_pinned_ids(
        pinned_command_ids.iter().map(String::as_str),
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

/// Toggle `command` in the pinned-command list and persist the new list.
///
/// Returns `true` when the command is pinned after the toggle and `false` when
/// it is unpinned. Save failures are non-fatal: the in-memory list remains
/// updated and the caller can continue rendering/dispatching normally.
pub(crate) fn toggle_command_palette_pinned_command(
    pinned_command_ids: &mut Vec<String>,
    pinned_path: &Path,
    command: &Command,
) -> bool {
    let command_id = command.diagnostic_id();
    let pinned_after_toggle =
        toggle_command_palette_pinned_command_id(pinned_command_ids, command_id.clone());
    if let Err(error) = save_command_palette_pinned_command_ids(pinned_path, pinned_command_ids) {
        tracing::debug!(
            target: "rge::editor-egui-host::command-palette",
            path = %pinned_path.display(),
            command_id = %command_id,
            error = %error,
            "command-palette pinned ids could not be saved"
        );
    }
    pinned_after_toggle
}

/// Toggle one diagnostic id in memory.
///
/// New pins are inserted at the front, so pinned ordering is most-recently
/// pinned first. Existing pins are removed.
pub(crate) fn toggle_command_palette_pinned_command_id(
    pinned_command_ids: &mut Vec<String>,
    command_id: String,
) -> bool {
    if let Some(position) = pinned_command_ids.iter().position(|id| id == &command_id) {
        pinned_command_ids.remove(position);
        return false;
    }
    pinned_command_ids.insert(0, command_id);
    pinned_command_ids.truncate(COMMAND_PALETTE_PINNED_COMMAND_LIMIT);
    true
}

fn parse_command_palette_pinned_ids(contents: &str) -> io::Result<Vec<String>> {
    normalize_command_palette_pinned_ids(contents.lines(), |index| index + 1)
}

fn normalize_command_palette_pinned_ids<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    line_number: impl Fn(usize) -> usize,
) -> io::Result<Vec<String>> {
    let mut normalized = Vec::new();
    for (index, id) in ids.into_iter().enumerate() {
        if !is_valid_command_palette_pinned_id(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid command-palette pinned diagnostic id on line {}",
                    line_number(index)
                ),
            ));
        }
        if !normalized.iter().any(|existing| existing == id) {
            normalized.push(id.to_owned());
            if normalized.len() == COMMAND_PALETTE_PINNED_COMMAND_LIMIT {
                break;
            }
        }
    }
    Ok(normalized)
}

fn is_valid_command_palette_pinned_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|ch| !ch.is_control() && !ch.is_whitespace())
}
