//! Source-hash → compiled-Module cache. First compile pays parse + codegen
//! + Cranelift JIT cost (≤2ms target per W19 spec); subsequent
//! [`Compiler::compile`] calls with the same source hit O(1) here.

use std::collections::HashMap;
use std::sync::Arc;

use wasmtime::{Engine, Module};

use crate::error::ExprError;

/// Cache key. We use the FxHash-style 64-bit hash of the source string;
/// collisions are unlikely at the scale of inline expressions (≤10⁴ unique
/// strings per editor session) and cheap to detect by re-comparing the
/// source on hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceHash(pub(crate) u64);

impl SourceHash {
    pub(crate) fn of(src: &str) -> Self {
        // Std DefaultHasher is sufficient — first-compile cost dwarfs
        // hashing time. Avoids pulling another crate in.
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        src.hash(&mut h);
        Self(h.finish())
    }
}

/// Compiled artifact stored in the cache. Cheap to clone (`Arc`).
#[derive(Clone, Debug)]
pub(crate) struct CachedArtifact {
    /// Source string, kept for collision verification.
    pub(crate) source: Arc<str>,
    /// Compiled WASM module — `wasmtime::Module` is internally `Arc`'d so
    /// cloning is cheap and shares the JIT'd code.
    pub(crate) module: Module,
    /// Variable schema: env-slot order. See [`crate::ast::Expr::walk_vars`].
    pub(crate) vars: Arc<[String]>,
}

/// Module cache. Indexed by source hash; entries are immutable once
/// inserted, so `&self` access suffices for callers that already hold the
/// cache by `&Arc<Mutex<...>>`.
pub(crate) struct ModuleCache {
    pub(crate) engine: Engine,
    map: HashMap<SourceHash, CachedArtifact>,
}

impl ModuleCache {
    pub(crate) fn new(engine: Engine) -> Self {
        Self {
            engine,
            map: HashMap::new(),
        }
    }

    /// Look up an artifact, returning `None` on miss or hash collision.
    pub(crate) fn lookup(&self, hash: SourceHash, src: &str) -> Option<CachedArtifact> {
        let a = self.map.get(&hash)?;
        if a.source.as_ref() == src {
            Some(a.clone())
        } else {
            None
        }
    }

    /// Compile from raw bytes and insert. Caller must have already done
    /// the parse + codegen and produced WASM bytes. Pre-condition: `src`
    /// hashes to `hash`.
    pub(crate) fn insert(
        &mut self,
        hash: SourceHash,
        src: &str,
        wasm_bytes: &[u8],
        vars: Vec<String>,
    ) -> Result<CachedArtifact, ExprError> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| ExprError::Wasmtime(format!("Module::new: {e}")))?;
        let artifact = CachedArtifact {
            source: Arc::from(src),
            module,
            vars: Arc::from(vars),
        };
        self.map.insert(hash, artifact.clone());
        Ok(artifact)
    }
}
