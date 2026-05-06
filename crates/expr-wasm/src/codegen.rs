//! AST → WASM bytes. Lowers an [`Expr`] into a single-function module
//! exporting `eval(env_ptr: i32) -> f32` plus a 1-page linear memory.
//!
//! Module layout:
//!
//! ```text
//! (module
//!   (type $unary  (func (param f32) (result f32)))
//!   (type $binary (func (param f32 f32) (result f32)))
//!   (type $eval   (func (param i32) (result f32)))
//!   (import "math" "sin"   (func (type $unary)))
//!   (import "math" "cos"   (func (type $unary)))
//!   ...
//!   (memory (export "memory") 1)
//!   (func (export "eval") (type $eval) (local f32 f32) ...)
//! )
//! ```
//!
//! Variables are addressed via `f32.load offset=index*4` from `env_ptr` and
//! the variable schema (name → index) is built once at compile time and
//! stored on [`crate::expr_handle::ExprHandle`].
//!
//! Comparison/logical ops produce `1.0` / `0.0` so the surface stays
//! single-typed; truthiness is `value != 0.0`.

use std::collections::HashMap;

use wasm_encoder::{
    BlockType, CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    ImportSection, Instruction, MemArg, MemorySection, MemoryType, Module, TypeSection, ValType,
};

use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::error::ExprError;
use crate::stdlib::{self, Lower};

/// Compiled WASM module bytes plus the variable schema.
pub struct Compiled {
    /// Raw `.wasm` bytes ready to feed into [`wasmtime::Module::new`].
    pub bytes: Vec<u8>,
    /// Variable names in env-slot order. Caller-supplied env slice indices
    /// must match this order.
    pub vars: Vec<String>,
}

// --- local slot layout ---
// 0: env_ptr (i32, function param)
// 1: scratch_a (f32) — used by `mod`, smoothstep `t`
// 2: scratch_b (f32) — used by `mod`
const ENV_PTR_LOCAL: u32 = 0;
const SCRATCH_A: u32 = 1;
const SCRATCH_B: u32 = 2;

/// Compile `expr` into WASM bytes.
///
/// # Errors
///
/// - [`ExprError::UnknownFunction`] — function not in [`stdlib::STDLIB`].
/// - [`ExprError::Arity`] — wrong number of args to a stdlib function.
/// - [`ExprError::Encode`] — internal codegen bug (should not surface).
pub fn compile(expr: &Expr) -> Result<Compiled, ExprError> {
    // 1. Collect variable schema in deterministic pre-order.
    let mut vars: Vec<String> = Vec::new();
    expr.walk_vars(&mut |name: &str| {
        if !vars.iter().any(|v| v == name) {
            vars.push(name.to_string());
        }
    });

    // 2. Build module sections.
    let mut module = Module::new();

    // -- types: 0=unary, 1=binary, 2=eval --
    // wasm-encoder 0.248: TypeSection no longer has a `.function(...)` method
    // directly; the encoder funnel is `types.ty().function(params, results)`.
    // Each `ty()` call mints one type slot.
    let mut types = TypeSection::new();
    types.ty().function([ValType::F32], [ValType::F32]); // 0
    types
        .ty()
        .function([ValType::F32, ValType::F32], [ValType::F32]); // 1
    types.ty().function([ValType::I32], [ValType::F32]); // 2
    module.section(&types);

    // -- imports: stdlib host functions in declaration order --
    let mut imports = ImportSection::new();
    let mut import_idx_by_name: HashMap<&'static str, u32> = HashMap::new();
    for (i, entry) in stdlib::imports().enumerate() {
        let type_idx: u32 = u32::from(entry.arity != 1);
        imports.import("math", entry.name, EntityType::Function(type_idx));
        import_idx_by_name.insert(
            entry.name,
            u32::try_from(i).expect("import idx fits in u32"),
        );
    }
    let import_count: u32 =
        u32::try_from(import_idx_by_name.len()).expect("import count fits in u32");
    module.section(&imports);

    // -- function declarations: just the eval function --
    let mut funcs = FunctionSection::new();
    funcs.function(2); // type index 2 = (i32) -> f32
    module.section(&funcs);

    // -- memory: 1 page exported --
    let mut mems = MemorySection::new();
    mems.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&mems);

    // -- exports --
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("eval", ExportKind::Func, import_count);
    module.section(&exports);

    // -- code --
    // Param 0: env_ptr (i32). Locals 1,2: f32 scratch slots.
    let mut func = Function::new([(2u32, ValType::F32)]);
    let ctx = CodegenCtx {
        vars: &vars,
        import_idx_by_name: &import_idx_by_name,
    };
    emit(&ctx, expr, &mut func)?;
    func.instruction(&Instruction::End);

    let mut code = CodeSection::new();
    code.function(&func);
    module.section(&code);

    Ok(Compiled {
        bytes: module.finish(),
        vars,
    })
}

struct CodegenCtx<'a> {
    vars: &'a [String],
    import_idx_by_name: &'a HashMap<&'static str, u32>,
}

fn emit(ctx: &CodegenCtx, expr: &Expr, f: &mut Function) -> Result<(), ExprError> {
    match expr {
        Expr::Number(n) => {
            // wasm-encoder 0.248: F32Const takes Ieee32, not f32. `From<f32>` is
            // implemented so `.into()` does the bit-cast.
            f.instruction(&Instruction::F32Const((*n).into()));
        }
        Expr::Var(name) => {
            // env_ptr_local; f32_load offset=index*4
            let idx = ctx
                .vars
                .iter()
                .position(|v| v == name)
                .expect("var collected by walk_vars must exist in schema");
            f.instruction(&Instruction::LocalGet(ENV_PTR_LOCAL));
            f.instruction(&Instruction::F32Load(MemArg {
                offset: (idx as u64) * 4,
                align: 2, // 2^2 = 4-byte alignment
                memory_index: 0,
            }));
        }
        Expr::Binary(op, lhs, rhs) => emit_binop(ctx, *op, lhs, rhs, f)?,
        Expr::Unary(op, arg) => {
            emit(ctx, arg, f)?;
            match op {
                UnaryOp::Neg => {
                    f.instruction(&Instruction::F32Neg);
                }
                UnaryOp::Not => {
                    // !x = (x == 0.0) as f32
                    f.instruction(&Instruction::F32Const(0.0_f32.into()));
                    f.instruction(&Instruction::F32Eq);
                    f.instruction(&Instruction::F32ConvertI32S);
                }
            }
        }
        Expr::Call(name, args) => emit_call(ctx, name, args, f)?,
        Expr::Ternary(cond, then_b, else_b) => {
            // if (cond != 0.0) { then } else { else }
            emit(ctx, cond, f)?;
            f.instruction(&Instruction::F32Const(0.0_f32.into()));
            f.instruction(&Instruction::F32Ne); // i32 truth
            f.instruction(&Instruction::If(BlockType::Result(ValType::F32)));
            emit(ctx, then_b, f)?;
            f.instruction(&Instruction::Else);
            emit(ctx, else_b, f)?;
            f.instruction(&Instruction::End);
        }
    }
    Ok(())
}

fn emit_binop(
    ctx: &CodegenCtx,
    op: BinaryOp,
    lhs: &Expr,
    rhs: &Expr,
    f: &mut Function,
) -> Result<(), ExprError> {
    match op {
        BinaryOp::And => {
            // (lhs != 0) && (rhs != 0) — produce f32 1.0/0.0
            emit(ctx, lhs, f)?;
            f.instruction(&Instruction::F32Const(0.0_f32.into()));
            f.instruction(&Instruction::F32Ne); // i32 truth
            emit(ctx, rhs, f)?;
            f.instruction(&Instruction::F32Const(0.0_f32.into()));
            f.instruction(&Instruction::F32Ne);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::F32ConvertI32S);
            return Ok(());
        }
        BinaryOp::Or => {
            emit(ctx, lhs, f)?;
            f.instruction(&Instruction::F32Const(0.0_f32.into()));
            f.instruction(&Instruction::F32Ne);
            emit(ctx, rhs, f)?;
            f.instruction(&Instruction::F32Const(0.0_f32.into()));
            f.instruction(&Instruction::F32Ne);
            f.instruction(&Instruction::I32Or);
            f.instruction(&Instruction::F32ConvertI32S);
            return Ok(());
        }
        BinaryOp::Mod => {
            // a % b = a - floor(a/b) * b
            emit(ctx, lhs, f)?;
            f.instruction(&Instruction::LocalSet(SCRATCH_A));
            emit(ctx, rhs, f)?;
            f.instruction(&Instruction::LocalSet(SCRATCH_B));
            // a
            f.instruction(&Instruction::LocalGet(SCRATCH_A));
            // floor(a / b) * b
            f.instruction(&Instruction::LocalGet(SCRATCH_A));
            f.instruction(&Instruction::LocalGet(SCRATCH_B));
            f.instruction(&Instruction::F32Div);
            f.instruction(&Instruction::F32Floor);
            f.instruction(&Instruction::LocalGet(SCRATCH_B));
            f.instruction(&Instruction::F32Mul);
            // a - floor(a/b)*b
            f.instruction(&Instruction::F32Sub);
            return Ok(());
        }
        _ => {}
    }

    emit(ctx, lhs, f)?;
    emit(ctx, rhs, f)?;
    let (inst, is_cmp) = match op {
        BinaryOp::Add => (Instruction::F32Add, false),
        BinaryOp::Sub => (Instruction::F32Sub, false),
        BinaryOp::Mul => (Instruction::F32Mul, false),
        BinaryOp::Div => (Instruction::F32Div, false),
        BinaryOp::Lt => (Instruction::F32Lt, true),
        BinaryOp::Le => (Instruction::F32Le, true),
        BinaryOp::Gt => (Instruction::F32Gt, true),
        BinaryOp::Ge => (Instruction::F32Ge, true),
        BinaryOp::Eq => (Instruction::F32Eq, true),
        BinaryOp::Ne => (Instruction::F32Ne, true),
        // already handled above
        BinaryOp::Mod | BinaryOp::And | BinaryOp::Or => {
            unreachable!("mod/and/or handled above")
        }
    };
    f.instruction(&inst);
    if is_cmp {
        // Comparison ops produce i32; convert to f32 1.0/0.0 for our truthy contract.
        f.instruction(&Instruction::F32ConvertI32S);
    }
    Ok(())
}

fn emit_call(
    ctx: &CodegenCtx,
    name: &str,
    args: &[Expr],
    f: &mut Function,
) -> Result<(), ExprError> {
    let entry = stdlib::lookup(name).ok_or_else(|| ExprError::UnknownFunction {
        name: name.to_string(),
    })?;
    if args.len() != entry.arity {
        return Err(ExprError::Arity {
            name: name.to_string(),
            expected: entry.arity,
            got: args.len(),
        });
    }
    match entry.lower {
        Lower::Import => {
            for a in args {
                emit(ctx, a, f)?;
            }
            let idx = *ctx.import_idx_by_name.get(name).expect("import declared");
            f.instruction(&Instruction::Call(idx));
        }
        Lower::Native => {
            for a in args {
                emit(ctx, a, f)?;
            }
            match name {
                "sqrt" => {
                    f.instruction(&Instruction::F32Sqrt);
                }
                "abs" => {
                    f.instruction(&Instruction::F32Abs);
                }
                "floor" => {
                    f.instruction(&Instruction::F32Floor);
                }
                "ceil" => {
                    f.instruction(&Instruction::F32Ceil);
                }
                "round" => {
                    // F32Nearest = round-half-to-even (IEEE rint).
                    f.instruction(&Instruction::F32Nearest);
                }
                "min" => {
                    f.instruction(&Instruction::F32Min);
                }
                "max" => {
                    f.instruction(&Instruction::F32Max);
                }
                "mod" => {
                    // mod(a, b) — same identity as `%` op, with the args
                    // already on the stack. Drop them, re-emit through
                    // the binop path which uses scratch locals.
                    f.instruction(&Instruction::Drop);
                    f.instruction(&Instruction::Drop);
                    emit_binop(ctx, BinaryOp::Mod, &args[0], &args[1], f)?;
                }
                _ => unreachable!("Native lowering missing for `{name}`"),
            }
        }
        Lower::Inline => match name {
            "clamp" => {
                // clamp(x, lo, hi) = min(max(x, lo), hi)
                emit(ctx, &args[0], f)?;
                emit(ctx, &args[1], f)?;
                f.instruction(&Instruction::F32Max);
                emit(ctx, &args[2], f)?;
                f.instruction(&Instruction::F32Min);
            }
            "lerp" => {
                // lerp(a, b, t) = a + (b - a) * t
                emit(ctx, &args[0], f)?; // a
                emit(ctx, &args[1], f)?; // a b
                emit(ctx, &args[0], f)?; // a b a
                f.instruction(&Instruction::F32Sub); // a (b-a)
                emit(ctx, &args[2], f)?; // a (b-a) t
                f.instruction(&Instruction::F32Mul); // a (b-a)*t
                f.instruction(&Instruction::F32Add); // a + (b-a)*t
            }
            "step" => {
                // GLSL step(edge, x) = x < edge ? 0.0 : 1.0
                // = 1.0 - (x < edge as f32)
                // Push 1.0 first so the final F32Sub computes 1.0 - <truth>.
                f.instruction(&Instruction::F32Const(1.0_f32.into()));
                emit(ctx, &args[1], f)?; // 1.0 x
                emit(ctx, &args[0], f)?; // 1.0 x edge
                f.instruction(&Instruction::F32Lt); // 1.0 i32_truth
                f.instruction(&Instruction::F32ConvertI32S); // 1.0 f32_truth
                f.instruction(&Instruction::F32Sub); // 1.0 - truth
            }
            "smoothstep" => {
                // smoothstep(e0, e1, x):
                //   t = clamp((x - e0) / (e1 - e0), 0, 1)
                //   return t * t * (3 - 2*t)
                //
                // Use SCRATCH_A for `t`. Note this collides with `mod` if
                // smoothstep contains `%` inside its arguments — but `mod`
                // saves both operands, computes, and leaves no residue, so
                // by the time we reach this point SCRATCH_A is free again.
                emit(ctx, &args[2], f)?; // x
                emit(ctx, &args[0], f)?; // e0
                f.instruction(&Instruction::F32Sub); // (x-e0)
                emit(ctx, &args[1], f)?; // e1
                emit(ctx, &args[0], f)?; // e0
                f.instruction(&Instruction::F32Sub); // (e1-e0)
                f.instruction(&Instruction::F32Div);
                f.instruction(&Instruction::F32Const(0.0_f32.into()));
                f.instruction(&Instruction::F32Max);
                f.instruction(&Instruction::F32Const(1.0_f32.into()));
                f.instruction(&Instruction::F32Min);
                f.instruction(&Instruction::LocalSet(SCRATCH_A)); // t -> SCRATCH_A
                                                                  // result = t * t * (3 - 2*t)
                f.instruction(&Instruction::LocalGet(SCRATCH_A));
                f.instruction(&Instruction::LocalGet(SCRATCH_A));
                f.instruction(&Instruction::F32Mul); // t²
                f.instruction(&Instruction::F32Const(3.0_f32.into()));
                f.instruction(&Instruction::LocalGet(SCRATCH_A));
                f.instruction(&Instruction::F32Const(2.0_f32.into()));
                f.instruction(&Instruction::F32Mul); // 2*t
                f.instruction(&Instruction::F32Sub); // 3 - 2*t
                f.instruction(&Instruction::F32Mul); // t² * (3 - 2*t)
            }
            _ => unreachable!("Inline lowering missing for `{name}`"),
        },
    }
    Ok(())
}
