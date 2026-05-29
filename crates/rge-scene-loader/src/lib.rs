//! `rge-scene-loader` — bridge from an `rge_data::Scene` into an
//! `rge_kernel_ecs::World`.
//!
//! Failure class: recoverable
//!
//! Narrow Scene-to-World bridge per GitHub issue #171. The caller parses an
//! `.rge-scene` file into an [`rge_data::Scene`]; this crate walks the scene
//! and lands every entity + component into a fresh
//! [`rge_kernel_ecs::World`].
//!
//! # Identity preservation
//!
//! Every [`rge_data::EntityId`] (a ULID) is converted via
//! [`rge_kernel_ecs::EntityId::from_ulid`] and spawned through
//! [`rge_kernel_ecs::World::spawn_with_id`] before any component is inserted,
//! so the scene's stable identity round-trips through the load.
//!
//! # Supported components
//!
//! The bridge is intentionally limited to the four simple-scene component
//! types named in issue #171:
//!
//! - `rge::components::Transform` → [`rge_components_spatial::Transform`]
//! - `rge::components::Camera`    → [`rge_components_render::Camera`]
//! - `rge::components::Light`     → [`rge_components_render::Light`]
//! - `rge::components::Visibility` → [`rge_components_visibility::Visibility`]
//!
//! Any other `ComponentValue.type_id` is surfaced as
//! [`SceneLoadError::UnsupportedComponent`] — unknown components are never
//! silently dropped.

use rge_components_render::{Camera, Light};
use rge_components_spatial::Transform;
use rge_components_visibility::Visibility;
use rge_data::{ComponentValue, Project, Scene};
use rge_kernel_ecs::{EntityId, World};

/// Errors that can occur while loading a [`Scene`] into a [`World`].
#[derive(Debug, thiserror::Error)]
pub enum SceneLoadError {
    /// A `ComponentValue` carried a `type_id` outside the supported set.
    #[error(
        "unsupported component type_id `{type_id}` on entity `{entity}` (loader supports only \
         Transform / Camera / Light / Visibility)"
    )]
    UnsupportedComponent {
        /// The unrecognized `type_id` string from the scene file.
        type_id: String,
        /// Canonical (26-char) ULID of the entity that carried the component.
        entity: String,
    },

    /// Typed RON deserialization of a `ComponentValue.data` payload failed.
    #[error("failed to deserialize component `{type_id}` on entity `{entity}` as RON: {source}")]
    Deserialize {
        /// The recognized component type_id the loader was decoding.
        type_id: String,
        /// Canonical (26-char) ULID of the entity that carried the component.
        entity: String,
        /// Underlying RON parse error.
        #[source]
        source: ron::de::SpannedError,
    },
}

/// Load `scene` into a fresh [`World`].
///
/// Spawns every scene entity with its original ULID, then walks each entity's
/// component envelope through a typed RON parse and inserts the resulting
/// component value through the typed [`World::insert`] API. Returns the
/// populated world, or a [`SceneLoadError`] on the first unsupported component
/// type_id or failed typed deserialization.
///
/// Scene relations and root-entity lists are **not** materialized — that
/// belongs to a future hierarchy / propagation pass and is out of scope for
/// this bridge.
///
/// # Errors
///
/// - [`SceneLoadError::UnsupportedComponent`] if any component carries a
///   `type_id` outside the four-string allowlist.
/// - [`SceneLoadError::Deserialize`] if a supported component's payload is
///   not valid RON for its target type.
pub fn load_scene_into_world(scene: &Scene) -> Result<World, SceneLoadError> {
    let mut world = World::new();

    // Spawn every entity first so later component insertions always target a
    // live entity, regardless of component-ordering quirks in the source file.
    for entity in &scene.entities {
        let ecs_id = EntityId::from_ulid(*entity.id.as_ulid());
        world.spawn_with_id(ecs_id);
    }

    for entity in &scene.entities {
        let ecs_id = EntityId::from_ulid(*entity.id.as_ulid());
        for component in &entity.components {
            insert_component(&mut world, ecs_id, &entity.id, component)?;
        }
    }

    Ok(world)
}

/// Errors from resolving a `.rge-project` / `.rge-scene` **path** into a
/// [`World`].
///
/// This is the path-level wrapper around [`load_scene_into_world`]: it adds the
/// file-read, RON-parse, and `.rge-project` scene-resolution failures that the
/// in-memory [`SceneLoadError`] boundary deliberately does not cover. The
/// underlying `Scene -> World` failure is preserved verbatim via
/// [`SceneWorldLoadError::Loader`] — this enum never broadens [`SceneLoadError`].
/// Messages are CLI-neutral; a binary caller (e.g. the editor's `--scene`
/// branch) supplies any flag framing.
#[derive(Debug, thiserror::Error)]
pub enum SceneWorldLoadError {
    /// File name was neither `.rge-project` nor `*.rge-scene`.
    #[error("{} has unsupported extension (expected .rge-project or .rge-scene)", .0.display())]
    UnsupportedExtension(std::path::PathBuf),
    /// Reading a `.rge-project` / `.rge-scene` file from disk failed.
    #[error("read {}: {source}", .path.display())]
    Read {
        /// The path that failed to read.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// RON parse of the `.rge-project` manifest failed.
    #[error("parse .rge-project {}: {source}", .path.display())]
    ParseProject {
        /// The `.rge-project` path that failed to parse.
        path: std::path::PathBuf,
        /// Underlying RON parse error.
        #[source]
        source: ron::de::SpannedError,
    },
    /// RON parse of a `.rge-scene` file failed.
    #[error("parse .rge-scene {}: {source}", .path.display())]
    ParseScene {
        /// The `.rge-scene` path that failed to parse.
        path: std::path::PathBuf,
        /// Underlying RON parse error.
        #[source]
        source: ron::de::SpannedError,
    },
    /// The `.rge-project` `scenes` list was empty (no scene to load).
    #[error(".rge-project {} has no scenes (expected at least one entry in `scenes`)", .0.display())]
    EmptyProjectScenes(std::path::PathBuf),
    /// The `.rge-project` path has no parent directory to resolve relative
    /// scene paths against (e.g. a bare filename).
    #[error(
        ".rge-project {} has no parent directory to resolve relative scene paths against",
        .0.display()
    )]
    ProjectHasNoParentDir(std::path::PathBuf),
    /// [`load_scene_into_world`] returned an error.
    #[error("load scene into world: {0}")]
    Loader(#[source] SceneLoadError),
}

/// Load a `.rge-project` (resolving its first scene relative to the project
/// directory) or a `.rge-scene` (parsed directly) into a fresh [`World`] via
/// [`load_scene_into_world`].
///
/// Dispatch is on [`std::path::Path::file_name`], NOT
/// [`std::path::Path::extension`]: the canonical project file name
/// `.rge-project` is a leading-dot-only name that Rust treats as having no
/// extension. A literal `.rge-project` is parsed as an [`Project`] and its
/// first `scenes` entry is resolved relative to the manifest's parent
/// directory; a `*.rge-scene` name is parsed directly as a [`Scene`]; any
/// other name is rejected as [`SceneWorldLoadError::UnsupportedExtension`].
///
/// Pure I/O + RON + loader call — no GPU, no winit — so it can be exercised
/// headlessly (see `tests/scene_path_loader.rs`).
///
/// # Errors
///
/// Returns a [`SceneWorldLoadError`] on unsupported extension, file-read
/// failure, RON parse failure, an empty project `scenes` list, a missing
/// project parent directory, or a wrapped [`SceneLoadError`] from the
/// `Scene -> World` load itself.
pub fn load_scene_world_from_path(path: &std::path::Path) -> Result<World, SceneWorldLoadError> {
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let kind = if file_name == ".rge-project" {
        "rge-project"
    } else if file_name.ends_with(".rge-scene") {
        "rge-scene"
    } else {
        ""
    };
    let scene: Scene = match kind {
        "rge-project" => {
            let raw = std::fs::read_to_string(path).map_err(|e| SceneWorldLoadError::Read {
                path: path.to_path_buf(),
                source: e,
            })?;
            let project: Project =
                ron::from_str(&raw).map_err(|e| SceneWorldLoadError::ParseProject {
                    path: path.to_path_buf(),
                    source: e,
                })?;
            let scene_rel = project
                .scenes
                .first()
                .ok_or_else(|| SceneWorldLoadError::EmptyProjectScenes(path.to_path_buf()))?;
            let project_dir = path
                .parent()
                .ok_or_else(|| SceneWorldLoadError::ProjectHasNoParentDir(path.to_path_buf()))?;
            let scene_path = project_dir.join(scene_rel.as_str());
            let scene_raw =
                std::fs::read_to_string(&scene_path).map_err(|e| SceneWorldLoadError::Read {
                    path: scene_path.clone(),
                    source: e,
                })?;
            ron::from_str(&scene_raw).map_err(|e| SceneWorldLoadError::ParseScene {
                path: scene_path,
                source: e,
            })?
        }
        "rge-scene" => {
            let raw = std::fs::read_to_string(path).map_err(|e| SceneWorldLoadError::Read {
                path: path.to_path_buf(),
                source: e,
            })?;
            ron::from_str(&raw).map_err(|e| SceneWorldLoadError::ParseScene {
                path: path.to_path_buf(),
                source: e,
            })?
        }
        _ => {
            return Err(SceneWorldLoadError::UnsupportedExtension(
                path.to_path_buf(),
            ))
        }
    };

    load_scene_into_world(&scene).map_err(SceneWorldLoadError::Loader)
}

/// Decode one `ComponentValue` and insert the resulting typed component into
/// `world` against `ecs_id`. The `scene_id` is used only for error reporting.
fn insert_component(
    world: &mut World,
    ecs_id: EntityId,
    scene_id: &rge_data::EntityId,
    component: &ComponentValue,
) -> Result<(), SceneLoadError> {
    match component.type_id.as_str() {
        "rge::components::Transform" => {
            let value = ron::from_str::<Transform>(&component.data).map_err(|source| {
                SceneLoadError::Deserialize {
                    type_id: component.type_id.clone(),
                    entity: scene_id.to_canonical(),
                    source,
                }
            })?;
            world.insert(ecs_id, value);
        }
        "rge::components::Camera" => {
            let value = ron::from_str::<Camera>(&component.data).map_err(|source| {
                SceneLoadError::Deserialize {
                    type_id: component.type_id.clone(),
                    entity: scene_id.to_canonical(),
                    source,
                }
            })?;
            world.insert(ecs_id, value);
        }
        "rge::components::Light" => {
            let value = ron::from_str::<Light>(&component.data).map_err(|source| {
                SceneLoadError::Deserialize {
                    type_id: component.type_id.clone(),
                    entity: scene_id.to_canonical(),
                    source,
                }
            })?;
            world.insert(ecs_id, value);
        }
        "rge::components::Visibility" => {
            let value = ron::from_str::<Visibility>(&component.data).map_err(|source| {
                SceneLoadError::Deserialize {
                    type_id: component.type_id.clone(),
                    entity: scene_id.to_canonical(),
                    source,
                }
            })?;
            world.insert(ecs_id, value);
        }
        other => {
            return Err(SceneLoadError::UnsupportedComponent {
                type_id: other.to_owned(),
                entity: scene_id.to_canonical(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rge_data::{Entity, SchemaVersion};

    use super::*;

    fn entity_with(type_id: &str, data: &str) -> Scene {
        let id = rge_data::EntityId::from_u128(0x1234);
        Scene {
            version: SchemaVersion::V0_1_0,
            name: "t".into(),
            entities: vec![Entity {
                id,
                name: "x".into(),
                components: vec![ComponentValue {
                    type_id: type_id.into(),
                    data: data.into(),
                }],
                relations: vec![],
            }],
            root_entities: vec![id],
        }
    }

    #[test]
    fn unsupported_component_errors() {
        let scene = entity_with("rge::components::Mystery", "()");
        let err = load_scene_into_world(&scene).expect_err("must reject unknown type_id");
        assert!(matches!(err, SceneLoadError::UnsupportedComponent { .. }));
    }

    #[test]
    fn malformed_payload_errors() {
        let scene = entity_with("rge::components::Visibility", "not-a-variant");
        let err = load_scene_into_world(&scene).expect_err("must reject bad RON");
        assert!(matches!(err, SceneLoadError::Deserialize { .. }));
    }

    #[test]
    fn empty_scene_yields_empty_world() {
        let scene = Scene::empty("blank", SchemaVersion::V0_1_0);
        let world = load_scene_into_world(&scene).expect("load");
        assert_eq!(world.entity_count(), 0);
    }
}
