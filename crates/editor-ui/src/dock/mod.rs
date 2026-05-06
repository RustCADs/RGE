//! `editor_ui::dock` — declarative dock-layout subsystem.
//!
//! Per PLAN.md §6.6 / W10 dispatch package. Builds on `egui_dock` and adapts the UE Slate
//! `FTabManager` / `FLayoutSaveRestore` / `FGlobalTabmanager::RegisterNomadTabSpawner` patterns
//! (no C++ source copied — see PLAN.md §1.3 Rule 2).
//!
//! Module map:
//!
//! - [`tab_id`]: stable [`TabId`](tab_id::TabId) newtype.
//! - [`version`]: layout-name versioning (`rge_main_v0.1.0`) and migration eligibility.
//! - [`tab_manager`]: declarative builder producing a [`LayoutBlueprint`](tab_manager::LayoutBlueprint).
//! - [`layout_service`]: persistence + blake3 tamper detection + version migration.
//! - [`spawner_registry`]: [`TabId`](tab_id::TabId) → factory closure map for tab bodies.

pub mod layout_service;
pub mod spawner_registry;
pub mod tab_id;
pub mod tab_manager;
pub mod version;

// Re-export the most-used types for downstream crates.
pub use layout_service::{
    LayoutFormat, LayoutLoadError, LayoutMigrationError, LayoutSaveError, LayoutService,
};
pub use spawner_registry::{
    register_default_spawners, PlaceholderTabBody, Spawner, SpawnerRegistry, DEFAULT_TAB_IDS,
};
pub use tab_id::TabId;
pub use tab_manager::{Direction, LayoutBlueprint, LayoutBuildError, LayoutNode, TabManager};
pub use version::{LayoutName, LayoutNameError, LayoutVersion};
