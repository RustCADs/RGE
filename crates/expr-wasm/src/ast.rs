//! AST for inline expressions.
//!
//! Closed grammar — see PLAN.md §5.3 + tasks/W19/PLAN.md. No statements,
//! no `let`, no loops, no closures. The grammar is intentionally narrow so
//! the codegen surface stays small and the whitelist (see [`crate::stdlib`])
//! cannot drift.

/// Binary operator over `f32`. Arithmetic ops produce `f32`; comparison /
/// logical ops produce `0.0` / `1.0` so the language stays single-typed at
/// the surface (truthy = non-zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%` — IEEE remainder (a - floor(a/b)*b).
    Mod,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `&&` — short-circuit at codegen time via `select`.
    And,
    /// `||`
    Or,
}

/// Unary operator over `f32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `-x`
    Neg,
    /// `!x` — logical not; `1.0` if `x == 0`, else `0.0`.
    Not,
}

/// Expression node. `Box`'d children keep the enum size flat; arena-style
/// storage was rejected as premature for ≤50-token expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// `42`, `3.14`, `0.7`. f32 by construction.
    Number(f32),
    /// `time`, `i`, `frequency`. Resolved against the env slice at
    /// codegen time — see [`crate::expr_handle::ExprHandle::vars`].
    Var(String),
    /// `lhs <op> rhs`.
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// `<op> arg`.
    Unary(UnaryOp, Box<Expr>),
    /// `name(arg, arg, ...)`. Whitelist enforced at codegen.
    Call(String, Vec<Expr>),
    /// `cond ? then_branch : else_branch`.
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Walk every [`Expr::Var`] node in the tree, in left-to-right pre-order.
    /// Used by [`crate::expr_handle::ExprHandle`] to build the variable
    /// schema once at compile time.
    pub fn walk_vars<'a>(&'a self, visit: &mut impl FnMut(&'a str)) {
        match self {
            Self::Number(_) => {}
            Self::Var(n) => visit(n.as_str()),
            Self::Binary(_, a, b) => {
                a.walk_vars(visit);
                b.walk_vars(visit);
            }
            Self::Unary(_, a) => a.walk_vars(visit),
            Self::Call(_, args) => {
                for a in args {
                    a.walk_vars(visit);
                }
            }
            Self::Ternary(c, t, e) => {
                c.walk_vars(visit);
                t.walk_vars(visit);
                e.walk_vars(visit);
            }
        }
    }
}
