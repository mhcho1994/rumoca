//! MLS §10.3.4 and SPEC_0040 SOLVE-C66: FMI reductions preserve operation
//! order, NaNs and signed zeros through the shared typed-function renderer.

use rumoca_ir_solve as solve;
use solve::{SolveReductionOperator as Reduction, SolveScalarType as Scalar};

use super::codegen_test_support::builtin_template;

fn program(operator: Reduction, scalar: Scalar, count: u32) -> solve::TypedProgram {
    let profile = solve::SolveArithmeticProfile::construct(
        match scalar {
            Scalar::Real { format } => format,
            _ => solve::SolveRealFormat::Binary64,
        },
        solve::SolveIntegerDomain::FULL,
    );
    let span = rumoca_core::Span::from_offsets(
        rumoca_core::SourceId::from_source_name("fmi_reduction.mo"),
        0,
        1,
    );
    solve::TypedProgram::construct(profile, |builder| {
        let input = builder.declare_slot(
            solve::SolveValueType::tensor(scalar, vec![count]).unwrap(),
            solve::SolveStorageClass::Input,
            solve::SolveSlotAccess::ReadOnly,
            span,
        )?;
        let output = builder.declare_slot(
            solve::SolveValueType::scalar(scalar),
            solve::SolveStorageClass::Output,
            solve::SolveSlotAccess::ReadWrite,
            span,
        )?;
        let value = builder.load(input, span)?;
        let reduced = builder.reduce(operator, value, span)?;
        builder.store(output, reduced, span)
    })
    .expect("checked reduction program")
}

fn render(program: &solve::TypedProgram, name: &str) -> Result<String, minijinja::Error> {
    let mut environment = super::create_environment();
    for (name, file) in [
        ("fmi-typed-arithmetic.jinja", "typed_arithmetic.jinja"),
        ("fmi-typed-tensors.jinja", "typed_tensors.jinja"),
        ("functions", "typed_functions.jinja"),
    ] {
        environment.add_template(name, builtin_template("fmi3", file))?;
    }
    environment.render_str(
        "{% from 'functions' import definition %}{{ definition(program, name) }}",
        minijinja::context! { program => program, name => name },
    )
}

const REAL: Scalar = Scalar::Real {
    format: solve::SolveRealFormat::Binary64,
};

#[test]
fn generated_reductions_execute_with_ordered_values_and_ieee_edges() {
    let mut source = String::from(
        "#include <assert.h>\n#include <math.h>\n#include <stdbool.h>\n\
         #include <stdint.h>\n#include <stddef.h>\n#include <string.h>\n",
    );
    source.push_str(
        &super::create_environment()
            .render_str("{{ real_extremum_prelude('c') }}", ())
            .unwrap(),
    );
    for (name, operator, scalar) in [
        ("sum3", Reduction::Sum, REAL),
        ("product3", Reduction::Product, REAL),
        ("min3", Reduction::Minimum, REAL),
        ("max3", Reduction::Maximum, REAL),
        ("all3", Reduction::All, Scalar::Boolean),
    ] {
        source.push_str(&render(&program(operator, scalar, 3), name).unwrap());
    }
    source.push_str(HARNESS);
    let work = tempfile::tempdir().unwrap();
    let path = work.path().join("reductions.c");
    let binary = work.path().join("reductions");
    std::fs::write(&path, source).unwrap();
    let compiled = std::process::Command::new("cc")
        .args([
            "-std=c11",
            "-O2",
            "-ffp-contract=off",
            "-Wall",
            "-Wextra",
            "-Werror",
        ])
        .arg(&path)
        .args(["-lm", "-o"])
        .arg(&binary)
        .output()
        .expect("start C compiler");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        std::process::Command::new(binary)
            .status()
            .unwrap()
            .success()
    );
}

const HARNESS: &str = r#"
#define RUN(fn, input) assert(fn((const void*[]){input}, (void*[]){&result}) == 0)
int main(void) {
    double result, ordinary[] = {5.0, 10.0, 15.0};
    RUN(sum3, ordinary); assert(result == 30.0);
    RUN(product3, ordinary); assert(result == 750.0);
    RUN(min3, ordinary); assert(result == 5.0);
    RUN(max3, ordinary); assert(result == 15.0);
    double ordered[] = {1e16, -1e16, 1.0};
    RUN(sum3, ordered); assert(result == 1.0);
    double zeros[] = {-0.0, -0.0, -0.0};
    RUN(sum3, zeros); assert(result == 0.0 && signbit(result));
    RUN(product3, zeros); assert(result == 0.0 && signbit(result));
    zeros[0] = 0.0;
    RUN(min3, zeros); assert(result == 0.0 && signbit(result));
    RUN(max3, zeros); assert(result == 0.0 && signbit(result));
    zeros[2] = 0.0;
    RUN(min3, zeros); assert(result == 0.0 && !signbit(result));
    RUN(max3, zeros); assert(result == 0.0 && !signbit(result));
    double nans[] = {NAN, NAN, NAN};
    RUN(min3, nans); assert(isnan(result));
    RUN(max3, nans); assert(isnan(result));
    nans[1] = 7.0;
    RUN(min3, nans); assert(result == 7.0);
    RUN(max3, nans); assert(result == 7.0);
    bool flags[] = {true, true, true}, all;
    assert(all3((const void*[]){flags}, (void*[]){&all}) == 0 && all);
    flags[1] = false;
    assert(all3((const void*[]){flags}, (void*[]){&all}) == 0 && !all);
    return 0;
}
"#;

#[test]
fn reductions_outside_the_supported_profile_fail_during_rendering() {
    for (operator, scalar, count) in [
        (
            Reduction::Sum,
            Scalar::Integer(solve::SolveIntegerDomain::FULL),
            3,
        ),
        (Reduction::Minimum, Scalar::Boolean, 3),
        (
            Reduction::Sum,
            Scalar::Real {
                format: solve::SolveRealFormat::Binary32,
            },
            3,
        ),
    ] {
        assert!(render(&program(operator, scalar, count), "unsupported").is_err());
    }
}
