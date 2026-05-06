//! `rge-components-networking` — networking marker components.
//!
//! Zero-cost at v1.0 per W01 PLAN: networking is a Reach feature (PLAN.md
//! §0.4 — "keep markers; defer impl"). The components let scene RON files
//! and gameplay code annotate intent today; the replication system in
//! `crates/replication` consumes them when the feature lands post-v1.
//!
//! State-only — see W01 PLAN exit criteria.

#![forbid(unsafe_code)]

mod authoritative;
mod network_owner;
mod peer_id;
mod remote_peer;
mod replicated;
mod replication_policy;

pub use authoritative::Authoritative;
pub use network_owner::NetworkOwner;
pub use peer_id::PeerId;
pub use remote_peer::RemotePeer;
pub use replicated::Replicated;
pub use replication_policy::ReplicationPolicy;
