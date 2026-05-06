//! `rge-editor-ui` — editor UI subsystem.
//!
//! Per [PLAN.md §6](../../PLAN.md). Adapts UE Slate patterns (UToolMenus, FTabManager,
//! FLayoutSaveRestore, FSlateStyleSet) to egui via `egui_dock`.

pub mod dock;
pub mod layout;
pub mod menus;
pub mod widgets;
pub mod workspace;
