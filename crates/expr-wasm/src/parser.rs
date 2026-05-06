//! Pratt parser for inline expressions. ~80 LOC including the lexer; the
//! Pratt core itself is ~30 LOC.
//!
//! Grammar (closed — see [`crate::ast`]):
//!
//! ```text
//! expr    := ternary
//! ternary := or ('?' expr ':' expr)?
//! or      := and ('||' and)*
//! and     := cmp ('&&' cmp)*
//! cmp     := sum (('<'|'<='|'>'|'>='|'=='|'!=') sum)*
//! sum     := mul (('+'|'-') mul)*
//! mul     := unary (('*'|'/'|'%') unary)*
//! unary   := ('-'|'!')? primary
//! primary := number | ident ('(' args ')')? | '(' expr ')'
//! ```

use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::error::ExprError;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f32),
    Ident(String),
    LParen,
    RParen,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    Bang,
    AmpAmp,
    PipePipe,
    Question,
    Colon,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, ExprError> {
    let bytes = src.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2 + 4);
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            b')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            b',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            b'+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            b'-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            b'*' => {
                out.push(Tok::Star);
                i += 1;
            }
            b'/' => {
                out.push(Tok::Slash);
                i += 1;
            }
            b'%' => {
                out.push(Tok::Percent);
                i += 1;
            }
            b'?' => {
                out.push(Tok::Question);
                i += 1;
            }
            b':' => {
                out.push(Tok::Colon);
                i += 1;
            }
            b'<' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push(Tok::Le);
                    i += 2;
                } else {
                    out.push(Tok::Lt);
                    i += 1;
                }
            }
            b'>' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push(Tok::Ge);
                    i += 2;
                } else {
                    out.push(Tok::Gt);
                    i += 1;
                }
            }
            b'=' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push(Tok::Eq);
                    i += 2;
                } else {
                    return Err(ExprError::Lex {
                        offset: i,
                        msg: "stray '=' (use '==' for equality)".into(),
                    });
                }
            }
            b'!' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push(Tok::Ne);
                    i += 2;
                } else {
                    out.push(Tok::Bang);
                    i += 1;
                }
            }
            b'&' => {
                if bytes.get(i + 1) == Some(&b'&') {
                    out.push(Tok::AmpAmp);
                    i += 2;
                } else {
                    return Err(ExprError::Lex {
                        offset: i,
                        msg: "stray '&' (use '&&' for logical and)".into(),
                    });
                }
            }
            b'|' => {
                if bytes.get(i + 1) == Some(&b'|') {
                    out.push(Tok::PipePipe);
                    i += 2;
                } else {
                    return Err(ExprError::Lex {
                        offset: i,
                        msg: "stray '|' (use '||' for logical or)".into(),
                    });
                }
            }
            b'0'..=b'9' | b'.' => {
                let start = i;
                while i < bytes.len()
                    && matches!(bytes[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    // Allow signed exponent: only consume `+`/`-` if previous char was `e`/`E`.
                    if matches!(bytes[i], b'+' | b'-')
                        && !(i > start && matches!(bytes[i - 1], b'e' | b'E'))
                    {
                        break;
                    }
                    i += 1;
                }
                let s = std::str::from_utf8(&bytes[start..i]).map_err(|_| ExprError::Lex {
                    offset: start,
                    msg: "non-utf8 number".into(),
                })?;
                let n: f32 = s.parse().map_err(|_| ExprError::Lex {
                    offset: start,
                    msg: format!("malformed number `{s}`"),
                })?;
                out.push(Tok::Num(n));
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = i;
                while i < bytes.len()
                    && matches!(bytes[i], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
                {
                    i += 1;
                }
                let s = std::str::from_utf8(&bytes[start..i]).map_err(|_| ExprError::Lex {
                    offset: start,
                    msg: "non-utf8 ident".into(),
                })?;
                out.push(Tok::Ident(s.to_string()));
            }
            other => {
                return Err(ExprError::Lex {
                    offset: i,
                    msg: format!("unexpected byte `{}` (0x{other:02x})", other as char),
                });
            }
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: Tok) -> Result<(), ExprError> {
        if self.eat(&t) {
            Ok(())
        } else {
            Err(ExprError::Parse(format!(
                "expected {t:?} got {:?}",
                self.peek()
            )))
        }
    }

    fn expr(&mut self) -> Result<Expr, ExprError> {
        let cond = self.binary(0)?;
        if self.eat(&Tok::Question) {
            let then_b = self.expr()?;
            self.expect(Tok::Colon)?;
            let else_b = self.expr()?;
            Ok(Expr::Ternary(
                Box::new(cond),
                Box::new(then_b),
                Box::new(else_b),
            ))
        } else {
            Ok(cond)
        }
    }

    /// Pratt loop. `min_bp` is the binding-power floor; we recurse with
    /// `op_bp + 1` to enforce left-associativity for all binary ops.
    fn binary(&mut self, min_bp: u8) -> Result<Expr, ExprError> {
        let mut lhs = self.unary()?;
        loop {
            let Some(op) = self.peek().and_then(infix_op) else {
                break;
            };
            let bp = infix_bp(op);
            if bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.binary(bp + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.bump();
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(self.unary()?)))
            }
            Some(Tok::Bang) => {
                self.bump();
                Ok(Expr::Unary(UnaryOp::Not, Box::new(self.unary()?)))
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, ExprError> {
        match self.bump() {
            Some(Tok::Num(n)) => Ok(Expr::Number(n)),
            Some(Tok::LParen) => {
                let e = self.expr()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    if !self.eat(&Tok::RParen) {
                        loop {
                            args.push(self.expr()?);
                            if self.eat(&Tok::RParen) {
                                break;
                            }
                            self.expect(Tok::Comma)?;
                        }
                    }
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            other => Err(ExprError::Parse(format!(
                "expected expression, got {other:?}"
            ))),
        }
    }
}

fn infix_op(t: &Tok) -> Option<BinaryOp> {
    Some(match t {
        Tok::Plus => BinaryOp::Add,
        Tok::Minus => BinaryOp::Sub,
        Tok::Star => BinaryOp::Mul,
        Tok::Slash => BinaryOp::Div,
        Tok::Percent => BinaryOp::Mod,
        Tok::Lt => BinaryOp::Lt,
        Tok::Le => BinaryOp::Le,
        Tok::Gt => BinaryOp::Gt,
        Tok::Ge => BinaryOp::Ge,
        Tok::Eq => BinaryOp::Eq,
        Tok::Ne => BinaryOp::Ne,
        Tok::AmpAmp => BinaryOp::And,
        Tok::PipePipe => BinaryOp::Or,
        _ => return None,
    })
}

/// Binding power. C-like: `||` < `&&` < cmp < `+/-` < `*/% `.
fn infix_bp(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Or => 1,
        BinaryOp::And => 2,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 3,
        BinaryOp::Eq | BinaryOp::Ne => 3,
        BinaryOp::Add | BinaryOp::Sub => 4,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 5,
    }
}

/// Parse `src` into an [`Expr`].
///
/// # Errors
///
/// - [`ExprError::Lex`] — invalid character or malformed numeric literal.
/// - [`ExprError::Parse`] — unexpected token or unbalanced grouping.
pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let toks = tokenize(src)?;
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr()?;
    if p.pos != p.toks.len() {
        return Err(ExprError::Parse(format!(
            "trailing tokens at position {}: {:?}",
            p.pos,
            p.peek()
        )));
    }
    Ok(e)
}
