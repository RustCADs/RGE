//! `rge-tool-architecture-lints` — CI architecture enforcement.
//!
//! Each subcommand runs one lint defined in `plans/PLAN.md` §1.3 / §1.8 / §1.14 / §1.15 /
//! §6.16. `all` runs every lint and exits non-zero if any failed.

#![forbid(unsafe_code)]

mod command_bus;
mod common;
mod editor_state_ownership;
mod failure_class;
mod forbidden_dep;
mod graph_foundation;
mod kernel_isolation;
mod no_utils;
mod projection_modules;
mod split_exemption;

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::common::{workspace_root, LintReport};

#[derive(Parser, Debug)]
#[command(
    name = "rge-tool-architecture-lints",
    about = "Architecture enforcement lints"
)]
struct Cli {
    /// Subcommand selecting which lint to run. `all` runs every lint.
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// PLAN.md §1.8: forbidden-dep DAG (Tier 1↛Tier 2, Tier 2↛Tier 3, cad-core stands alone, etc.).
    ForbiddenDep,
    /// PLAN.md §1.3 Rule 3: any `.rs` >1000 lines requires a `// SPLIT-EXEMPTION:` annotation.
    SplitExemption,
    /// PLAN.md §1.3 Rule 3: no `utils.rs` / `helpers.rs` files allowed.
    NoUtils,
    /// PLAN.md §1.14: no crate may define its own `NodeId` / `EdgeId` / `StableHash` outside `kernel/graph-foundation`.
    GraphFoundation,
    /// PLAN.md §1.15: `Selection` / `Hover` / `ActiveTool` / `ModalState` / `DragDrop` may only be defined in `crates/editor-state`,
    /// and `editor-state` may not import authoritative content types (coordination-not-authority).
    EditorStateOwnership,
    /// PLAN.md §6.16: direct world-mutation API imports outside `crates/editor-actions` (active enforcement since Phase 2 — see command_bus.rs module doc).
    CommandBus,
    /// PLAN.md §1.8 / §1.6: `projection_structural` cannot import `projection_runtime` or `projection_editor`.
    ProjectionModules,
    /// PLAN.md §1.6.4: each binary asset format has exactly one `io-*` import path.
    KernelIsolation,
    /// PLAN.md §1.13: every Tier-1 + Tier-2 crate must declare its failure class in lib.rs.
    FailureClass,
    /// Run every lint above and aggregate the results.
    All,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let root = workspace_root()?;
    println!("workspace: {}", root.display());

    let reports: Vec<LintReport> = match cli.cmd {
        Cmd::ForbiddenDep => vec![forbidden_dep::run(&root)?],
        Cmd::SplitExemption => vec![split_exemption::run(&root)?],
        Cmd::NoUtils => vec![no_utils::run(&root)?],
        Cmd::GraphFoundation => vec![graph_foundation::run(&root)?],
        Cmd::EditorStateOwnership => vec![editor_state_ownership::run(&root)?],
        Cmd::CommandBus => vec![command_bus::run(&root)?],
        Cmd::ProjectionModules => vec![projection_modules::run(&root)?],
        Cmd::KernelIsolation => vec![kernel_isolation::run(&root)?],
        Cmd::FailureClass => vec![failure_class::run(&root)?],
        Cmd::All => vec![
            forbidden_dep::run(&root)?,
            split_exemption::run(&root)?,
            no_utils::run(&root)?,
            graph_foundation::run(&root)?,
            editor_state_ownership::run(&root)?,
            command_bus::run(&root)?,
            projection_modules::run(&root)?,
            kernel_isolation::run(&root)?,
            failure_class::run(&root)?,
        ],
    };

    let mut all_ok = true;
    for r in &reports {
        if !r.print() {
            all_ok = false;
        }
    }
    Ok(all_ok)
}
