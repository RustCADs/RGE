//! `TabManager` — declarative dock-layout builder over [`egui_dock`].
//!
//! UE Slate parallel: `FTabManager` + `FTabManager::FStack` / `FTabManager::FSplitter` builder
//! types in `Engine/Source/Runtime/Slate/Public/Framework/Docking/TabManager.h`. The builder
//! grammar adapted here is structurally the same — a tree of `PrimaryArea ⊃ Splitter ⊃ Stack ⊃
//! Tab` — but rendered through immediate-mode egui via `egui_dock` instead of Slate. No C++
//! source was copied; only the externally visible builder grammar.
//!
//! ## Grammar (per W10 dispatch package)
//!
//! ```text
//! TabManager::new_layout("rge_main_v0.1.0")
//!     .new_primary_area(Direction::Horizontal, 1.0)
//!         .new_splitter(Direction::Vertical, 0.7)
//!             .new_stack()
//!                 .add_tab("viewport")
//!                 .add_tab("scene_panel")
//!             .done()
//!         .done()
//!         .new_stack()
//!             .add_tab("property_panel")
//!         .done()
//!     .done()
//! .build();
//! ```
//!
//! `done()` pops the current scope. `build()` consumes the manager and produces a
//! [`LayoutBlueprint`].
//!
//! ## Why a blueprint, not a `DockState` directly
//!
//! `egui_dock::DockState` is *not* `Serialize`-stable across versions — its on-disk schema is
//! internally indexed (`NodeIndex` / `Vec<Surface>`), making cross-version migration brittle.
//! The `LayoutBlueprint` keeps the human-meaningful tree intact so:
//!
//! 1. [`super::layout_service`] persists it as RON (canonical) or JSON (debug).
//! 2. [`super::version`] migration diffs two blueprints by `TabId` (preserving geometry for
//!    unchanged tabs).
//! 3. The actual `DockState<T>` is reconstructed lazily on load via
//!    [`LayoutBlueprint::into_dock_state_with`].

use std::str::FromStr;

use egui_dock::{DockState, NodeIndex, Split};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::dock::tab_id::TabId;
use crate::dock::version::{LayoutName, LayoutNameError};

/// Splitter axis.
///
/// `Horizontal` = side-by-side panes (split on X). `Vertical` = stacked top/bottom (split on Y).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Direction {
    /// Children laid out left-to-right.
    Horizontal,
    /// Children laid out top-to-bottom.
    Vertical,
}

impl Direction {
    /// Map this `Direction` to the [`egui_dock::Split`] used to *introduce* a second child:
    /// `Horizontal` ⇒ split right (new child appears to the right), `Vertical` ⇒ split below.
    /// Mirror axes (`Left` / `Above`) are not used by the builder; we always append to the
    /// right/below so child ordering matches blueprint declaration order.
    #[must_use]
    pub const fn split_to_extend(self) -> Split {
        match self {
            Direction::Horizontal => Split::Right,
            Direction::Vertical => Split::Below,
        }
    }
}

/// One node of a layout blueprint.
///
/// Trees of `LayoutNode` are produced by [`TabManager::build`] and consumed by both the layout
/// service (for persistence) and `into_dock_state_with` (for runtime display).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LayoutNode {
    /// A horizontal/vertical splitter. `fraction` ∈ [0,1] is the size of the *first* child as a
    /// share of the parent. Children are laid out in order according to `direction`.
    Splitter {
        /// Axis on which children are laid out.
        direction: Direction,
        /// First-child share of parent area. Clamped to `[0.05, 0.95]` on materialization.
        fraction: f32,
        /// Children, in display order.
        children: Vec<LayoutNode>,
    },
    /// A leaf containing one or more tabs (a tab-stack). The first tab is the active tab on
    /// fresh build; the layout service preserves the active-tab choice on reload.
    Stack {
        /// Tabs in this stack, in display order.
        tabs: Vec<TabId>,
    },
}

impl LayoutNode {
    /// Walk the tree, collecting every `TabId` referenced. Used by version-migration to compute
    /// added/removed/unchanged sets.
    pub fn collect_tab_ids(&self, out: &mut Vec<TabId>) {
        match self {
            LayoutNode::Splitter { children, .. } => {
                for c in children {
                    c.collect_tab_ids(out);
                }
            }
            LayoutNode::Stack { tabs } => out.extend(tabs.iter().cloned()),
        }
    }

    /// Filter the tree to only contain the supplied set of tabs (preserve geometry of every
    /// surviving stack/splitter). Returns `None` if filtering would empty the tree entirely.
    ///
    /// Used by [`super::layout_service::LayoutService::migrate`].
    pub fn retain_tabs(&self, keep: &std::collections::HashSet<TabId>) -> Option<LayoutNode> {
        match self {
            LayoutNode::Splitter {
                direction,
                fraction,
                children,
            } => {
                let mut new_children = Vec::with_capacity(children.len());
                for c in children {
                    if let Some(filtered) = c.retain_tabs(keep) {
                        new_children.push(filtered);
                    }
                }
                if new_children.is_empty() {
                    None
                } else if new_children.len() == 1 {
                    // Collapse single-child splitters to avoid degenerate intermediate nodes.
                    new_children.pop()
                } else {
                    Some(LayoutNode::Splitter {
                        direction: *direction,
                        fraction: *fraction,
                        children: new_children,
                    })
                }
            }
            LayoutNode::Stack { tabs } => {
                let kept: Vec<TabId> = tabs.iter().filter(|t| keep.contains(t)).cloned().collect();
                if kept.is_empty() {
                    None
                } else {
                    Some(LayoutNode::Stack { tabs: kept })
                }
            }
        }
    }
}

/// Top-level layout description: name + root node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutBlueprint {
    /// Versioned layout name, e.g. `rge_main_v0.1.0`.
    pub name: LayoutName,
    /// Root of the dock tree. Always a `Splitter` (the primary area is rendered as a single-
    /// direction splitter wrapping its sole child).
    pub root: LayoutNode,
}

impl LayoutBlueprint {
    /// All tab ids referenced anywhere in the layout, in tree-walk order.
    #[must_use]
    pub fn collect_tab_ids(&self) -> Vec<TabId> {
        let mut out = Vec::new();
        self.root.collect_tab_ids(&mut out);
        out
    }

    /// Materialize this blueprint into an [`egui_dock::DockState<T>`], invoking the supplied
    /// closure to construct each tab's `T` from its [`TabId`].
    ///
    /// The `materialize` closure is the bridge to [`super::spawner_registry::SpawnerRegistry`] in
    /// production; tests can pass any closure that produces the test's `Tab` type.
    ///
    /// The geometry of the produced `DockState` walks the blueprint top-down:
    /// 1. The first leaf encountered seeds `DockState::new`.
    /// 2. Subsequent leaves are added via `Tree::split_*` against the leaf produced by their
    ///    *parent* splitter.
    /// 3. `fraction` is clamped to `[0.05, 0.95]` to keep egui_dock's invariants happy.
    ///
    /// # Errors
    /// Returns `LayoutBuildError::EmptyStack` if a stack with zero tabs is encountered (this can
    /// only happen if the blueprint was constructed by hand bypassing the builder, or by a
    /// faulty migration).
    pub fn into_dock_state_with<T, F>(
        self,
        mut materialize: F,
    ) -> Result<DockState<T>, LayoutBuildError>
    where
        F: FnMut(&TabId) -> T,
    {
        // The blueprint root is always a Splitter wrapping a primary-area child. We unwrap one
        // level to drop the synthetic primary area before building the tree.
        let primary_child = match self.root {
            LayoutNode::Splitter { children, .. } if children.len() == 1 => {
                children.into_iter().next().unwrap()
            }
            other => other,
        };

        // Find the first stack so we have something to seed DockState::new with.
        let layout_name_str = self.name.to_string();
        let leaves = collect_leaves(&primary_child);
        let Some((first_path, first_stack)) = leaves.first() else {
            return Err(LayoutBuildError::EmptyStack(layout_name_str));
        };

        if first_stack.is_empty() {
            return Err(LayoutBuildError::EmptyStack(layout_name_str));
        }

        let first_tabs: Vec<T> = first_stack.iter().map(&mut materialize).collect();
        let mut dock = DockState::<T>::new(first_tabs);

        // For each subsequent leaf, walk back up its path until we hit the LCA with the
        // already-placed leaf, then split off the appropriate node. This preserves the
        // declarative geometry: a leaf placed under a Splitter at depth N inherits N levels of
        // splitting from its ancestors.
        //
        // adapted from egui_dock 0.12 doctests on 2026-05-05 — `Tree::split_*` family signatures.
        let mut path_to_node: Vec<(PathPrefix, NodeIndex)> =
            vec![(first_path.clone(), NodeIndex::root())];

        for (path, stack_tabs) in leaves.iter().skip(1) {
            // Find the deepest existing leaf whose path is a prefix of `path` and whose next
            // step in `path` says where to split.
            let parent_idx = find_parent_node(&path_to_node, path);
            let parent_split_step = path[parent_idx.depth()];
            let parent_node = parent_idx.node;
            let parent_direction = parent_split_step.direction;
            let parent_fraction = clamp_fraction(parent_split_step.fraction);
            let new_tabs: Vec<T> = stack_tabs.iter().map(&mut materialize).collect();

            // The geometry rule: when extending, the existing content stays in place and the new
            // child is placed in the requested direction. Use split_right for Horizontal and
            // split_below for Vertical so child ordering matches blueprint order.
            let surface = dock.main_surface_mut();
            let new_indices = match (parent_direction, parent_split_step.is_first_child_existing) {
                // existing tree IS the first child → new tab goes to right/below
                (Direction::Horizontal, true) => {
                    surface.split_right(parent_node, parent_fraction, new_tabs)
                }
                (Direction::Vertical, true) => {
                    surface.split_below(parent_node, parent_fraction, new_tabs)
                }
                // existing tree is the second child → new tab goes to left/above so the
                // declared first-child fraction stays accurate
                (Direction::Horizontal, false) => {
                    surface.split_left(parent_node, parent_fraction, new_tabs)
                }
                (Direction::Vertical, false) => {
                    surface.split_above(parent_node, parent_fraction, new_tabs)
                }
            };
            // After split: [old_node, new_node]. Record the new leaf's index.
            path_to_node.push((path.clone(), new_indices[1]));
        }

        Ok(dock)
    }
}

/// Errors produced when building a layout.
#[derive(Debug, Error)]
pub enum LayoutBuildError {
    /// The supplied layout name did not parse via [`LayoutName::parse`].
    #[error("invalid layout name: {0}")]
    InvalidName(#[from] LayoutNameError),
    /// `build()` was called without ever opening a primary area.
    #[error("layout `{0}` has no primary area")]
    NoPrimaryArea(String),
    /// `build()` was called with one or more open scopes still pending `.done()`.
    #[error("layout `{0}` has {1} unclosed scopes")]
    UnclosedScopes(String, usize),
    /// A `Stack` was finalized empty (no `add_tab` calls).
    #[error("empty stack in layout `{0}`")]
    EmptyStack(String),
    /// A `Splitter` was finalized with zero children.
    #[error("empty splitter in layout `{0}`")]
    EmptySplitter(String),
    /// Misuse: `add_tab` was called outside of a stack scope.
    #[error("add_tab called outside a stack scope in layout `{0}`")]
    AddTabOutsideStack(String),
    /// Misuse: a non-tab node was attached as a stack child.
    #[error("stack scope cannot contain non-tab children in layout `{0}`")]
    NonTabInStack(String),
}

/// Internal scope kind tracked while the user constructs the layout.
#[derive(Debug)]
enum Scope {
    /// Primary-area shell — wraps a single child, captured in `child`.
    PrimaryArea {
        direction: Direction,
        fraction: f32,
        child: Option<LayoutNode>,
    },
    Splitter {
        direction: Direction,
        fraction: f32,
        children: Vec<LayoutNode>,
    },
    Stack {
        tabs: Vec<TabId>,
    },
}

/// Declarative dock-layout builder.
///
/// Errors are deferred to [`build`](Self::build) so the call chain stays composable;
/// pre-validation (e.g. layout-name parse failure) is captured in `pending_error` and short-
/// circuited from `build`.
pub struct TabManager {
    name_str: String,
    name: Option<LayoutName>,
    pending_error: Option<LayoutBuildError>,
    /// Open scopes (innermost last). Empty after the primary area is closed.
    scopes: Vec<Scope>,
    /// Set when the primary area is closed.
    completed_root: Option<LayoutNode>,
}

impl TabManager {
    /// Begin a new layout with the given versioned name (e.g. `rge_main_v0.1.0`).
    ///
    /// Layout-name parsing is eager — any [`LayoutNameError`] is stashed and surfaced by
    /// [`build`](Self::build).
    #[must_use]
    pub fn new_layout(name: impl Into<String>) -> Self {
        let name_str = name.into();
        let (name, pending_error) = match LayoutName::from_str(&name_str) {
            Ok(parsed) => (Some(parsed), None),
            Err(e) => (None, Some(LayoutBuildError::InvalidName(e))),
        };
        Self {
            name_str,
            name,
            pending_error,
            scopes: Vec::new(),
            completed_root: None,
        }
    }

    /// Open the **primary area** — exactly one of these is allowed per layout. Subsequent
    /// `.new_splitter()` / `.new_stack()` calls populate the area's child.
    #[must_use]
    pub fn new_primary_area(mut self, direction: Direction, fraction: f32) -> Self {
        if self.completed_root.is_some() {
            self.pending_error
                .get_or_insert_with(|| LayoutBuildError::UnclosedScopes(self.name_str.clone(), 0));
            return self;
        }
        self.scopes.push(Scope::PrimaryArea {
            direction,
            fraction,
            child: None,
        });
        self
    }

    /// Open a splitter scope as a child of the current scope.
    #[must_use]
    pub fn new_splitter(mut self, direction: Direction, fraction: f32) -> Self {
        self.scopes.push(Scope::Splitter {
            direction,
            fraction,
            children: Vec::new(),
        });
        self
    }

    /// Open a tab-stack scope as a child of the current scope.
    #[must_use]
    pub fn new_stack(mut self) -> Self {
        self.scopes.push(Scope::Stack { tabs: Vec::new() });
        self
    }

    /// Add a tab to the current `Stack` scope. Misuse outside a stack scope is captured for
    /// reporting from [`build`](Self::build).
    #[must_use]
    pub fn add_tab(mut self, id: impl Into<TabId>) -> Self {
        let id = id.into();
        match self.scopes.last_mut() {
            Some(Scope::Stack { tabs }) => tabs.push(id),
            _ => {
                debug_assert!(false, "TabManager::add_tab called outside a Stack scope");
                if self.pending_error.is_none() {
                    self.pending_error =
                        Some(LayoutBuildError::AddTabOutsideStack(self.name_str.clone()));
                }
            }
        }
        self
    }

    /// Close the innermost open scope, attaching its produced [`LayoutNode`] to its parent (or
    /// to `completed_root` if it was the primary area).
    #[must_use]
    pub fn done(mut self) -> Self {
        let Some(scope) = self.scopes.pop() else {
            if self.pending_error.is_none() {
                self.pending_error =
                    Some(LayoutBuildError::UnclosedScopes(self.name_str.clone(), 0));
            }
            return self;
        };

        let node = match scope {
            Scope::PrimaryArea {
                direction,
                fraction,
                child,
            } => {
                let Some(child) = child else {
                    self.pending_error.get_or_insert_with(|| {
                        LayoutBuildError::EmptySplitter(self.name_str.clone())
                    });
                    return self;
                };
                LayoutNode::Splitter {
                    direction,
                    fraction,
                    children: vec![child],
                }
            }
            Scope::Splitter {
                direction,
                fraction,
                children,
            } => {
                if children.is_empty() {
                    self.pending_error.get_or_insert_with(|| {
                        LayoutBuildError::EmptySplitter(self.name_str.clone())
                    });
                    return self;
                }
                LayoutNode::Splitter {
                    direction,
                    fraction,
                    children,
                }
            }
            Scope::Stack { tabs } => {
                if tabs.is_empty() {
                    self.pending_error
                        .get_or_insert_with(|| LayoutBuildError::EmptyStack(self.name_str.clone()));
                    return self;
                }
                LayoutNode::Stack { tabs }
            }
        };

        // Attach to parent (or stash as completed root).
        if let Some(parent) = self.scopes.last_mut() {
            match parent {
                Scope::PrimaryArea { child, .. } => {
                    if child.is_some() {
                        self.pending_error.get_or_insert_with(|| {
                            LayoutBuildError::EmptySplitter(self.name_str.clone())
                        });
                    } else {
                        *child = Some(node);
                    }
                }
                Scope::Splitter { children, .. } => children.push(node),
                Scope::Stack { .. } => {
                    debug_assert!(false, "non-tab node attached to a Stack");
                    self.pending_error.get_or_insert_with(|| {
                        LayoutBuildError::NonTabInStack(self.name_str.clone())
                    });
                }
            }
        } else {
            // Outermost scope just closed.
            if self.completed_root.is_some() {
                self.pending_error.get_or_insert_with(|| {
                    LayoutBuildError::UnclosedScopes(self.name_str.clone(), 0)
                });
            } else {
                self.completed_root = Some(node);
            }
        }
        self
    }

    /// Finalize the builder and produce a [`LayoutBlueprint`].
    ///
    /// # Errors
    /// - [`LayoutBuildError::InvalidName`] if the constructor's name failed to parse.
    /// - [`LayoutBuildError::UnclosedScopes`] if any scope is still open.
    /// - [`LayoutBuildError::NoPrimaryArea`] if no primary area was ever opened+closed.
    /// - Whatever pending error was stashed during construction (empty stacks/splitters,
    ///   `add_tab` outside a stack, etc).
    pub fn build(self) -> Result<LayoutBlueprint, LayoutBuildError> {
        if let Some(err) = self.pending_error {
            return Err(err);
        }
        if !self.scopes.is_empty() {
            return Err(LayoutBuildError::UnclosedScopes(
                self.name_str,
                self.scopes.len(),
            ));
        }
        let Some(root) = self.completed_root else {
            return Err(LayoutBuildError::NoPrimaryArea(self.name_str));
        };
        let name = self.name.ok_or_else(|| {
            LayoutBuildError::InvalidName(LayoutNameError::MissingSuffix(self.name_str.clone()))
        })?;
        Ok(LayoutBlueprint { name, root })
    }
}

// =============================================================================
// Internal: blueprint → DockState materialization
// =============================================================================

/// One step in the path from blueprint root to a leaf.
///
/// `Copy` so it can be plucked out of a `Vec<PathStep>` by index without an explicit clone —
/// every field is itself `Copy`.
#[derive(Clone, Copy, Debug)]
struct PathStep {
    direction: Direction,
    fraction: f32,
    /// True if the child we descended into was the *first* child of its splitter (i.e. the
    /// existing geometry stays as the first child and the new tabs go right/below).
    is_first_child_existing: bool,
}

/// Path from blueprint root to a leaf. Index 0 is the outermost step.
type PathPrefix = Vec<PathStep>;

/// Pair: (path-from-root, &Stack tabs).
fn collect_leaves(root: &LayoutNode) -> Vec<(PathPrefix, Vec<TabId>)> {
    let mut out = Vec::new();
    walk(root, &mut Vec::new(), &mut out);
    out
}

fn walk(node: &LayoutNode, path: &mut PathPrefix, out: &mut Vec<(PathPrefix, Vec<TabId>)>) {
    match node {
        LayoutNode::Stack { tabs } => out.push((path.clone(), tabs.clone())),
        LayoutNode::Splitter {
            direction,
            fraction,
            children,
        } => {
            // For each child, push a step; for non-first children we encode that the *existing*
            // sibling occupies the first-child slot.
            for (i, child) in children.iter().enumerate() {
                let step = PathStep {
                    direction: *direction,
                    fraction: *fraction,
                    is_first_child_existing: i != 0,
                };
                if i == 0 {
                    // First child: descent inherits the parent's slot, so don't push a step
                    // (the leaf created here will be split-from when sibling i=1 arrives).
                    walk(child, path, out);
                } else {
                    path.push(step);
                    walk(child, path, out);
                    path.pop();
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct PathPos {
    node: NodeIndex,
    depth_from_root: usize,
}

impl PathPos {
    fn depth(&self) -> usize {
        self.depth_from_root
    }
}

/// Find the existing leaf whose path is the longest prefix of `target_path`. The split for the
/// new leaf is performed against this node's index.
fn find_parent_node(placed: &[(PathPrefix, NodeIndex)], target_path: &[PathStep]) -> PathPos {
    // The set is small (one entry per leaf placed so far), so a linear scan with a longest-
    // prefix-match heuristic is fine.
    let mut best: Option<(usize, NodeIndex)> = None;
    for (path, idx) in placed {
        let common = common_prefix_len(path, target_path);
        // We want: path is a prefix of target_path (i.e. common == path.len()) AND we minimize
        // common == path.len() (most-recent leaf at the right depth).
        if common == path.len() {
            match best {
                Some((d, _)) if d >= common => {}
                _ => best = Some((common, *idx)),
            }
        }
    }
    let (depth, node) = best.expect("at least one leaf has already been placed");
    PathPos {
        node,
        depth_from_root: depth,
    }
}

fn common_prefix_len(a: &[PathStep], b: &[PathStep]) -> usize {
    let mut i = 0;
    while i < a.len() && i < b.len() && step_eq(&a[i], &b[i]) {
        i += 1;
    }
    i
}

fn step_eq(a: &PathStep, b: &PathStep) -> bool {
    a.direction == b.direction
        && a.is_first_child_existing == b.is_first_child_existing
        && (a.fraction - b.fraction).abs() < f32::EPSILON
}

fn clamp_fraction(f: f32) -> f32 {
    f.clamp(0.05, 0.95)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::tab_id::TabId;

    fn three_pane_layout() -> LayoutBlueprint {
        // [ viewport | scene_panel ]   ← top horizontal split, 0.7 to viewport
        // [    console + log      ]   ← bottom stack, 0.3
        TabManager::new_layout("rge_main_v0.1.0")
            .new_primary_area(Direction::Vertical, 1.0)
            .new_splitter(Direction::Vertical, 0.7)
            .new_splitter(Direction::Horizontal, 0.7)
            .new_stack()
            .add_tab("viewport")
            .done()
            .new_stack()
            .add_tab("scene_panel")
            .done()
            .done()
            .new_stack()
            .add_tab("console")
            .add_tab("log")
            .done()
            .done()
            .done()
            .build()
            .expect("layout builds")
    }

    #[test]
    fn declarative_builder_collects_all_tabs() {
        let bp = three_pane_layout();
        let ids: Vec<String> = bp
            .collect_tab_ids()
            .into_iter()
            .map(TabId::into_string)
            .collect();
        assert_eq!(ids, vec!["viewport", "scene_panel", "console", "log"]);
        assert_eq!(bp.name.to_string(), "rge_main_v0.1.0");
    }

    #[test]
    fn declarative_builder_produces_dock_state() {
        let bp = three_pane_layout();
        let dock = bp
            .into_dock_state_with(|id: &TabId| id.as_str().to_owned())
            .expect("materializes");
        // tree should contain all 4 tabs across multiple leaves
        let main = dock.main_surface();
        let total: usize = main
            .iter()
            .map(|node| match node {
                egui_dock::Node::Leaf(leaf) => leaf.tabs.len(),
                _ => 0,
            })
            .sum();
        assert_eq!(total, 4, "all four tabs end up somewhere in the tree");
    }

    #[test]
    fn rejects_invalid_layout_name() {
        let err = TabManager::new_layout("missing-suffix")
            .build()
            .unwrap_err();
        assert!(matches!(err, LayoutBuildError::InvalidName(_)));
    }

    #[test]
    fn rejects_unclosed_scopes() {
        let err = TabManager::new_layout("rge_main_v0.1.0")
            .new_primary_area(Direction::Vertical, 1.0)
                .new_stack()
                    .add_tab("viewport")
            // missing two `.done()` calls
            .build()
            .unwrap_err();
        assert!(matches!(err, LayoutBuildError::UnclosedScopes(_, _)));
    }

    #[test]
    fn rejects_no_primary_area() {
        let err = TabManager::new_layout("rge_main_v0.1.0")
            .build()
            .unwrap_err();
        assert!(matches!(err, LayoutBuildError::NoPrimaryArea(_)));
    }

    #[test]
    fn rejects_empty_stack() {
        let err = TabManager::new_layout("rge_main_v0.1.0")
            .new_primary_area(Direction::Vertical, 1.0)
            .new_stack()
            .done()
            .done()
            .build()
            .unwrap_err();
        assert!(matches!(err, LayoutBuildError::EmptyStack(_)));
    }

    #[test]
    fn rejects_empty_splitter() {
        let err = TabManager::new_layout("rge_main_v0.1.0")
            .new_primary_area(Direction::Vertical, 1.0)
            .new_splitter(Direction::Horizontal, 0.5)
            .done()
            .done()
            .build()
            .unwrap_err();
        assert!(matches!(err, LayoutBuildError::EmptySplitter(_)));
    }

    #[test]
    fn retain_tabs_drops_missing_and_collapses_singleton_splitters() {
        let bp = three_pane_layout();
        let keep: std::collections::HashSet<TabId> = ["viewport", "console"]
            .iter()
            .map(|s| TabId::new(*s))
            .collect();
        let filtered = bp.root.retain_tabs(&keep).expect("non-empty after filter");
        let mut ids = Vec::new();
        filtered.collect_tab_ids(&mut ids);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&TabId::new("viewport")));
        assert!(ids.contains(&TabId::new("console")));
        assert!(!ids.contains(&TabId::new("scene_panel")));
    }

    #[test]
    fn single_stack_layout_round_trips_through_dock_state() {
        let bp = TabManager::new_layout("rge_main_v0.1.0")
            .new_primary_area(Direction::Vertical, 1.0)
            .new_stack()
            .add_tab("only_tab")
            .done()
            .done()
            .build()
            .unwrap();
        let dock = bp
            .into_dock_state_with(|t: &TabId| t.as_str().to_owned())
            .unwrap();
        assert_eq!(dock.main_surface().num_tabs(), 1);
    }
}
