//! Numerical correctness — every compiled expression must agree with a
//! straight f64 reference implementation within 1e-6 (W19 spec exit
//! criterion).

use rge_expr_wasm::{Compiler, Evaluator};

const EPS: f64 = 1.0e-6;

fn run(src: &str, vars: &[(&str, f32)]) -> f32 {
    let compiler = Compiler::new();
    let handle = compiler.compile(src).expect("compile");
    let mut env = vec![0.0_f32; handle.vars().len()];
    for (i, slot) in handle.vars().iter().enumerate() {
        let v = vars
            .iter()
            .find(|(k, _)| k == slot)
            .unwrap_or_else(|| panic!("missing var `{slot}` in test env"))
            .1;
        env[i] = v;
    }
    let mut eval = Evaluator::new(&compiler, handle).expect("instantiate");
    eval.eval(&env).expect("eval")
}

fn approx(a: f32, b: f64) {
    let d = (a as f64 - b).abs();
    assert!(d < EPS, "got {a}, expected ≈{b}, |Δ|={d} > {EPS}");
}

#[test]
fn arithmetic_basic() {
    approx(run("1 + 2 * 3", &[]), 7.0);
    approx(run("(1 + 2) * 3", &[]), 9.0);
    approx(run("-3 + 5", &[]), 2.0);
    approx(run("10 / 4", &[]), 2.5);
    approx(run("10 - 4 - 2", &[]), 4.0); // left-assoc
}

#[test]
fn modulo() {
    approx(run("10 % 3", &[]), 1.0);
    approx(run("mod(10, 3)", &[]), 1.0);
    approx(run("7.5 % 2", &[]), 1.5);
}

#[test]
fn variables_and_calls() {
    approx(
        run("sin(time * 0.5) * 0.3 + 0.7", &[("time", 1.5)]),
        (1.5_f64 * 0.5).sin() * 0.3 + 0.7,
    );
    approx(run("sqrt(16)", &[]), 4.0);
    approx(run("pow(2, 10)", &[]), 1024.0);
    approx(run("atan2(1, 1)", &[]), std::f64::consts::FRAC_PI_4);
}

#[test]
fn comparison_and_logical() {
    approx(run("1 < 2", &[]), 1.0);
    approx(run("1 > 2", &[]), 0.0);
    approx(run("1 == 1", &[]), 1.0);
    approx(run("1 != 1", &[]), 0.0);
    approx(run("(1 < 2) && (3 < 4)", &[]), 1.0);
    approx(run("(1 < 2) && (3 > 4)", &[]), 0.0);
    approx(run("(1 > 2) || (3 < 4)", &[]), 1.0);
    approx(run("!0", &[]), 1.0);
    approx(run("!1", &[]), 0.0);
}

#[test]
fn ternary() {
    approx(run("1 < 2 ? 10 : 20", &[]), 10.0);
    approx(run("1 > 2 ? 10 : 20", &[]), 20.0);
    approx(run("x < 0 ? -x : x", &[("x", -3.0)]), 3.0_f64);
    approx(run("x < 0 ? -x : x", &[("x", 5.0)]), 5.0);
}

#[test]
fn range_funcs() {
    approx(run("clamp(5, 0, 10)", &[]), 5.0);
    approx(run("clamp(-1, 0, 10)", &[]), 0.0);
    approx(run("clamp(15, 0, 10)", &[]), 10.0);
    approx(run("min(3, 5)", &[]), 3.0);
    approx(run("max(3, 5)", &[]), 5.0);
    approx(run("lerp(0, 100, 0.25)", &[]), 25.0);
    approx(run("step(0.5, 0.4)", &[]), 0.0);
    approx(run("step(0.5, 0.6)", &[]), 1.0);
    // smoothstep(e0, e1, x) at midpoint should give 0.5
    approx(run("smoothstep(0, 1, 0.5)", &[]), 0.5);
    // smoothstep clamps below e0 → 0
    approx(run("smoothstep(0, 1, -0.5)", &[]), 0.0);
    // smoothstep clamps above e1 → 1
    approx(run("smoothstep(0, 1, 1.5)", &[]), 1.0);
}

#[test]
fn rounding_funcs() {
    approx(run("floor(3.7)", &[]), 3.0);
    approx(run("ceil(3.2)", &[]), 4.0);
    approx(run("abs(-3.5)", &[]), 3.5);
    approx(run("round(3.5)", &[]), 4.0); // ties to even, then 4
    approx(run("round(4.5)", &[]), 4.0); // ties to even
}

#[test]
fn precedence_chain() {
    // 1 + 2 * 3 < 10 ? sin(0) + 1 : 99
    //  = 7 < 10 ? 0 + 1 : 99 = 1
    approx(run("1 + 2 * 3 < 10 ? sin(0) + 1 : 99", &[]), 1.0);
}

#[test]
fn multi_var_schema_order() {
    let compiler = Compiler::new();
    // Vars discovered in pre-order: a, b, c
    let h = compiler.compile("a * b + c").expect("compile");
    assert_eq!(h.vars(), &["a", "b", "c"]);
    let mut eval = Evaluator::new(&compiler, h).expect("inst");
    let r = eval.eval(&[2.0, 3.0, 1.0]).expect("eval");
    approx(r, 7.0);
}
