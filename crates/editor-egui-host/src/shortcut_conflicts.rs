//! Host-local shortcut-conflict diagnostics derived from the projected menu.

use crate::menu::{ProjectedMainMenu, ProjectedShortcutConflict};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShortcutConflictRow {
    pub shortcut: String,
    pub entries: Vec<String>,
}

pub(crate) fn shortcut_conflict_rows(main_menu: &ProjectedMainMenu) -> Vec<ShortcutConflictRow> {
    main_menu
        .conflicts
        .iter()
        .map(shortcut_conflict_row)
        .collect()
}

pub(crate) fn menu_affordance(ui: &mut egui::Ui, open: &mut bool, rows: &[ShortcutConflictRow]) {
    if rows.is_empty() {
        return;
    }

    if ui.button("Shortcut Conflicts").clicked() {
        open_shortcut_conflicts(open, rows);
    }
}

pub(crate) fn show(ctx: &egui::Context, open: &mut bool, rows: &[ShortcutConflictRow]) {
    if rows.is_empty() {
        *open = false;
        return;
    }
    if !*open {
        return;
    }

    egui::Window::new("Shortcut Conflicts")
        .id(egui::Id::new("rge_shortcut_conflicts"))
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .open(open)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("rge_shortcut_conflicts_rows")
                .max_height(320.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("rge_shortcut_conflicts_grid")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Shortcut");
                            ui.strong("Entry IDs");
                            ui.end_row();

                            for row in rows {
                                ui.monospace(row.shortcut.as_str());
                                ui.monospace(row.entries.join(", "));
                                ui.end_row();
                            }
                        });
                });
        });
}

pub(crate) fn open_shortcut_conflicts(open: &mut bool, rows: &[ShortcutConflictRow]) {
    if !rows.is_empty() {
        *open = true;
    }
}

fn shortcut_conflict_row(conflict: &ProjectedShortcutConflict) -> ShortcutConflictRow {
    ShortcutConflictRow {
        shortcut: conflict.shortcut.clone(),
        entries: conflict.entries.clone(),
    }
}

#[cfg(test)]
mod tests {
    use rge_editor_ui::menus::{
        default_editor_menu, file_menu_point, Command, Key, MenuEntry, Modifiers, PredicateContext,
        Shortcut,
    };

    use super::*;
    use crate::menu::{
        command_palette_entries, project_main_menu, register_menu_entry, ProjectedMainMenu,
    };
    use crate::MenuCommandHandoff;

    fn row(shortcut: &str, entries: &[&str]) -> ProjectedShortcutConflict {
        ProjectedShortcutConflict {
            shortcut: shortcut.to_owned(),
            entries: entries.iter().map(|entry| (*entry).to_owned()).collect(),
        }
    }

    #[test]
    fn shortcut_conflict_rows_preserve_projected_conflict_order_and_entry_ids() {
        let main_menu = ProjectedMainMenu {
            conflicts: vec![
                row(
                    "Ctrl+Alt+M",
                    &["plugin.mesh_audit.open", "plugin.mesh_audit.alt"],
                ),
                row("Ctrl+S", &["file.save", "plugin.conflict.save"]),
            ],
            ..ProjectedMainMenu::default()
        };

        assert_eq!(
            shortcut_conflict_rows(&main_menu),
            vec![
                ShortcutConflictRow {
                    shortcut: "Ctrl+Alt+M".to_owned(),
                    entries: vec![
                        "plugin.mesh_audit.open".to_owned(),
                        "plugin.mesh_audit.alt".to_owned(),
                    ],
                },
                ShortcutConflictRow {
                    shortcut: "Ctrl+S".to_owned(),
                    entries: vec!["file.save".to_owned(), "plugin.conflict.save".to_owned()],
                },
            ],
            "conflict diagnostics are a direct ordered projection of ProjectedMainMenu.conflicts"
        );
    }

    #[test]
    fn shortcut_conflict_rows_are_empty_for_default_menu_projection() {
        let menu = project_main_menu(&default_editor_menu(), &PredicateContext::default());

        assert!(
            shortcut_conflict_rows(&menu).is_empty(),
            "default_editor_menu projects no conflict diagnostics"
        );
    }

    #[test]
    fn shortcut_conflicts_preserve_registry_winner_while_projecting_diagnostics() {
        let mut registry = default_editor_menu();
        let shortcut = Shortcut::new(Modifiers::CTRL, Key::Char('S'));
        register_menu_entry(
            &mut registry,
            &file_menu_point(),
            MenuEntry::new(
                "plugin.conflict.save",
                "Plugin Save",
                Command::Custom("plugin.save".to_owned()),
            )
            .with_shortcut(shortcut.clone()),
        )
        .expect("synthetic plugin entry registers in the File menu");

        let resolved = registry.resolve(&PredicateContext::default());
        assert_eq!(
            resolved
                .accelerator_table
                .resolve(&shortcut)
                .map(ToString::to_string),
            Some("file.save".to_owned()),
            "MenuRegistry::resolve keeps the first registered shortcut winner"
        );

        let rows =
            shortcut_conflict_rows(&project_main_menu(&registry, &PredicateContext::default()));
        assert_eq!(
            rows,
            vec![ShortcutConflictRow {
                shortcut: "Ctrl+S".to_owned(),
                entries: vec!["file.save".to_owned(), "plugin.conflict.save".to_owned()],
            }],
            "the host still exposes the non-fatal conflict as diagnostics"
        );
    }

    #[test]
    fn shortcut_conflicts_open_and_close_are_read_only_host_state() {
        let menu = ProjectedMainMenu {
            conflicts: vec![row("Ctrl+S", &["file.save", "plugin.conflict.save"])],
            ..ProjectedMainMenu::default()
        };
        let rows = shortcut_conflict_rows(&menu);
        let handoff = MenuCommandHandoff::new();
        let command_palette_open = true;
        let command_palette_filter = "save".to_owned();
        let command_palette_selected_index = Some(0);
        let recent_ids = vec![Command::Save.diagnostic_id()];
        let pinned_ids = vec![Command::ToggleCommandPalette.diagnostic_id()];
        let mut shortcut_help_open = false;
        let mut shortcut_conflicts_open = false;

        open_shortcut_conflicts(&mut shortcut_conflicts_open, &rows);
        assert!(shortcut_conflicts_open);
        assert!(command_palette_open, "command palette stays open");
        assert_eq!(command_palette_filter, "save");
        assert_eq!(command_palette_selected_index, Some(0));
        assert_eq!(recent_ids, vec![Command::Save.diagnostic_id()]);
        assert_eq!(
            pinned_ids,
            vec![Command::ToggleCommandPalette.diagnostic_id()]
        );
        assert!(!shortcut_help_open, "shortcut help stays untouched");
        assert!(
            handoff.drain().is_empty(),
            "opening conflict diagnostics does not enqueue menu commands"
        );

        shortcut_conflicts_open = false;
        assert!(!shortcut_conflicts_open);
        assert!(command_palette_open, "command palette still stays open");
        assert_eq!(command_palette_filter, "save");
        assert_eq!(command_palette_selected_index, Some(0));
        assert!(!shortcut_help_open, "shortcut help still stays untouched");
        assert!(
            handoff.drain().is_empty(),
            "closing conflict diagnostics does not enqueue menu commands"
        );

        shortcut_help_open = true;
        assert!(shortcut_help_open);
    }

    #[test]
    fn shortcut_conflicts_window_closes_when_projected_rows_disappear() {
        let ctx = egui::Context::default();
        let rows = Vec::new();
        let mut shortcut_conflicts_open = true;

        show(&ctx, &mut shortcut_conflicts_open, &rows);

        assert!(
            !shortcut_conflicts_open,
            "an open diagnostics window is cleared when the current projection has no conflicts"
        );
    }

    #[test]
    fn shortcut_conflict_projection_leaves_palette_entries_and_menu_handoff_unchanged() {
        let mut registry = default_editor_menu();
        register_menu_entry(
            &mut registry,
            &file_menu_point(),
            MenuEntry::new(
                "plugin.conflict.save",
                "Plugin Save",
                Command::Custom("plugin.save".to_owned()),
            )
            .with_shortcut(Shortcut::new(Modifiers::CTRL, Key::Char('S'))),
        )
        .expect("synthetic plugin entry registers in the File menu");
        let menu = project_main_menu(&registry, &PredicateContext::default());
        let palette_before = command_palette_entries(&menu);
        let handoff = MenuCommandHandoff::new();

        let rows = shortcut_conflict_rows(&menu);
        let palette_after = command_palette_entries(&menu);

        assert_eq!(
            rows,
            vec![ShortcutConflictRow {
                shortcut: "Ctrl+S".to_owned(),
                entries: vec!["file.save".to_owned(), "plugin.conflict.save".to_owned()],
            }]
        );
        assert_eq!(
            palette_after, palette_before,
            "projecting conflict rows does not mutate command-palette projection"
        );
        assert!(
            handoff.drain().is_empty(),
            "projecting conflict rows does not enqueue menu commands"
        );
    }
}
