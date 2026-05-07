//! Public API: `Compiler`, `Evaluator`, `ExprHandle`. Per W19 spec:
//!
//! ```ignore
//! Compiler::compile(&str) -> Result<ExprHandle>
//! Evaluator::eval(handle, env) -> f32
//! ```
//!
//! Two-tier design lets the editor keep one `Compiler` (owns the cache)
//! per-process while individual subsystems (material-graph, anim-graph)
//! own short-lived `Evaluator`s for the actual call site. Eval is the hot
//! path; compile is amortized.

use std::sync::{Arc, Mutex};

use wasmtime::{Caller, Engine, Linker, Memory, Store, TypedFunc};

use crate::cache::{ModuleCache, SourceHash};
use crate::error::ExprError;
use crate::{codegen, parser, stdlib};

/// Opaque handle to a compiled expression. Carries the variable schema
/// (so callers can map their data into the right env slots) and an Arc
/// to the cached artifact so the underlying [`wasmtime::Module`] survives
/// even if the [`Compiler`] is dropped.
#[derive(Clone, Debug)]
pub struct ExprHandle {
    pub(crate) artifact: crate::cache::CachedArtifact,
}

impl ExprHandle {
    /// Variable names in env-slot order. Caller must populate the env
    /// slice they pass to [`Evaluator::eval`] in the same order.
    #[must_use]
    pub fn vars(&self) -> &[String] {
        &self.artifact.vars
    }

    /// Original source string. Useful for diagnostics.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.artifact.source
    }
}

/// Compiles inline expressions to wasmtime modules. Holds the engine and
/// the module cache. Cheap to clone (`Arc<Mutex<...>>` inside).
#[derive(Clone)]
pub struct Compiler {
    cache: Arc<Mutex<ModuleCache>>,
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiler {
    /// New compiler with a fresh wasmtime [`Engine`]. The Engine config
    /// uses Cranelift defaults — wasmtime workspace pin enables the
    /// `cranelift` and `cache` features (see workspace `Cargo.toml`).
    #[must_use]
    pub fn new() -> Self {
        let engine = Engine::default();
        Self {
            cache: Arc::new(Mutex::new(ModuleCache::new(engine))),
        }
    }

    /// Underlying wasmtime engine. Exposed so [`Evaluator`] can build
    /// a [`Store`] sharing the same engine (required for Module reuse).
    ///
    /// # Panics
    ///
    /// Panics if another thread panicked while holding the cache mutex
    /// (poisoned mutex). expr-wasm never deliberately holds the lock
    /// across user code so this should not occur in practice.
    #[must_use]
    pub fn engine(&self) -> Engine {
        // Engine is internally `Arc`'d; clone is cheap.
        self.cache
            .lock()
            .expect("cache mutex poisoned")
            .engine
            .clone()
    }

    /// Compile `src` into an [`ExprHandle`].
    ///
    /// On cache hit (same source hashed previously), returns in O(1).
    /// On miss, runs parser → codegen → wasmtime [`Module::new`].
    ///
    /// # Errors
    ///
    /// - [`ExprError::Lex`] / [`ExprError::Parse`] — malformed source.
    /// - [`ExprError::UnknownFunction`] — non-whitelisted function call.
    /// - [`ExprError::Arity`] — wrong arg count to a stdlib call.
    /// - [`ExprError::Wasmtime`] — module rejected by wasmtime (should
    ///   not occur outside of codegen bugs).
    ///
    /// # Panics
    ///
    /// Panics if the cache mutex is poisoned (only possible if another
    /// thread panicked while holding it).
    pub fn compile(&self, src: &str) -> Result<ExprHandle, ExprError> {
        let hash = SourceHash::of(src);
        // Fast path: cache hit.
        {
            let cache = self.cache.lock().expect("cache mutex poisoned");
            if let Some(a) = cache.lookup(hash, src) {
                return Ok(ExprHandle { artifact: a });
            }
        }
        // Miss: parse + codegen + insert.
        let ast = parser::parse(src)?;
        let compiled = codegen::compile(&ast)?;
        let mut cache = self.cache.lock().expect("cache mutex poisoned");
        let artifact = cache.insert(hash, src, &compiled.bytes, compiled.vars)?;
        Ok(ExprHandle { artifact })
    }
}

/// Per-handle wasmtime instance + memory + cached typed func. One
/// [`Evaluator`] services exactly one [`ExprHandle`] — sharing across
/// handles would require switching memory layouts per call which defeats
/// the cached-fast-path.
pub struct Evaluator {
    store: Store<()>,
    memory: Memory,
    eval_fn: TypedFunc<i32, f32>,
    handle: ExprHandle,
}

impl Evaluator {
    /// Instantiate `handle` against `compiler`'s engine. The Linker wires
    /// the stdlib transcendental imports (sin, cos, …) to the host f32
    /// implementations.
    ///
    /// # Errors
    ///
    /// [`ExprError::Wasmtime`] if instantiation or linker resolution
    /// fails. This indicates a codegen/whitelist mismatch (bug).
    pub fn new(compiler: &Compiler, handle: ExprHandle) -> Result<Self, ExprError> {
        let engine = compiler.engine();
        let mut store = Store::new(&engine, ());
        let mut linker = Linker::new(&engine);
        register_imports(&mut linker)?;
        let instance = linker
            .instantiate(&mut store, &handle.artifact.module)
            .map_err(|e| ExprError::Wasmtime(format!("instantiate: {e}")))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| ExprError::Wasmtime("module did not export `memory`".into()))?;
        let eval_fn = instance
            .get_typed_func::<i32, f32>(&mut store, "eval")
            .map_err(|e| ExprError::Wasmtime(format!("get_typed_func eval: {e}")))?;
        Ok(Self {
            store,
            memory,
            eval_fn,
            handle,
        })
    }

    /// The bound handle.
    #[must_use]
    pub fn handle(&self) -> &ExprHandle {
        &self.handle
    }

    /// Evaluate against `env`. The slice must be at least
    /// `handle.vars().len()` long; values must be f32 in the order
    /// declared by [`ExprHandle::vars`].
    ///
    /// Hot path: ~5 ns per call after the first. The cost split is
    /// roughly memory-write x `N_vars` + one wasmtime call dispatch.
    ///
    /// # Errors
    ///
    /// - [`ExprError::ShortEnv`] if `env.len() < handle.vars().len()`.
    /// - [`ExprError::Wasmtime`] on trap (division by zero is silently
    ///   `inf`/`nan` per WASM spec, not a trap).
    pub fn eval(&mut self, env: &[f32]) -> Result<f32, ExprError> {
        let n = self.handle.artifact.vars.len();
        if env.len() < n {
            return Err(ExprError::ShortEnv {
                expected: n,
                got: env.len(),
            });
        }
        // Write env values to memory at offset 0 as little-endian f32s.
        // Wasmtime memory writes go through the Store; we batch into a
        // single write via memory.data_mut().
        let bytes_needed = n * 4;
        let data = self.memory.data_mut(&mut self.store);
        // SAFETY pre-condition: page size ≥ 64 KiB ≫ N_vars * 4 for
        // realistic expressions. We declared minimum=1 (1 page = 64 KiB)
        // so up to 16 384 vars fit. If somehow exceeded, eval will fault
        // on the dst_data range check below.
        let dst = &mut data[..bytes_needed];
        for (i, v) in env.iter().take(n).enumerate() {
            dst[i * 4..(i + 1) * 4].copy_from_slice(&v.to_le_bytes());
        }
        // Call into wasm. env_ptr = 0.
        self.eval_fn
            .call(&mut self.store, 0)
            .map_err(|e| ExprError::Wasmtime(format!("call eval: {e}")))
    }
}

/// Wire stdlib transcendental imports to host f32 math. Closed set —
/// matches [`stdlib::imports`].
fn register_imports(linker: &mut Linker<()>) -> Result<(), ExprError> {
    // Each import is named "math::<fn>" and has signature determined by arity.
    // wasmtime panics on signature mismatch; we trust the codegen here.
    for entry in stdlib::imports() {
        match (entry.name, entry.arity) {
            ("sin", 1) => bind_unary(linker, "sin", f32::sin)?,
            ("cos", 1) => bind_unary(linker, "cos", f32::cos)?,
            ("tan", 1) => bind_unary(linker, "tan", f32::tan)?,
            ("asin", 1) => bind_unary(linker, "asin", f32::asin)?,
            ("acos", 1) => bind_unary(linker, "acos", f32::acos)?,
            ("atan", 1) => bind_unary(linker, "atan", f32::atan)?,
            ("exp", 1) => bind_unary(linker, "exp", f32::exp)?,
            ("log", 1) => bind_unary(linker, "log", f32::ln)?,
            ("log2", 1) => bind_unary(linker, "log2", f32::log2)?,
            ("atan2", 2) => bind_binary(linker, "atan2", f32::atan2)?,
            ("pow", 2) => bind_binary(linker, "pow", f32::powf)?,
            (other, _) => {
                return Err(ExprError::Wasmtime(format!(
                    "stdlib import `{other}` declared but not bound — codegen/linker mismatch"
                )));
            }
        }
    }
    Ok(())
}

fn bind_unary(linker: &mut Linker<()>, name: &str, f: fn(f32) -> f32) -> Result<(), ExprError> {
    linker
        .func_wrap("math", name, move |_caller: Caller<'_, ()>, x: f32| f(x))
        .map_err(|e| ExprError::Wasmtime(format!("func_wrap math::{name}: {e}")))?;
    Ok(())
}

fn bind_binary(
    linker: &mut Linker<()>,
    name: &str,
    f: fn(f32, f32) -> f32,
) -> Result<(), ExprError> {
    linker
        .func_wrap(
            "math",
            name,
            move |_caller: Caller<'_, ()>, x: f32, y: f32| f(x, y),
        )
        .map_err(|e| ExprError::Wasmtime(format!("func_wrap math::{name}: {e}")))?;
    Ok(())
}
