//! The derived numerical program is not on the wire (Execution IR v2, D1/D2).
//!
//! Grep-based because the property is about which names exist in which file,
//! not about which values a function returns: a reintroduced `observations`
//! field in the wire module would compile and pass every behavioural test
//! while putting one lowering's output back into a public artifact.
use std::fs;

use super::architecture_hardening_support::workspace_root;

/// Files that define the bytes an `.rbc` carries.
const WIRE_MODULES: [&str; 2] = [
    "crates/rumoca-bitcode/src/schema.rs",
    "crates/rumoca-ir-solve/src/execution.rs",
];

/// Names that belong to the derived program and nowhere near the wire.
const DERIVED_ONLY: [&str; 5] = [
    "observations",
    "algebraic_blocks",
    "initial_blocks",
    "NumericalProgram",
    "ScalarOp",
];

#[test]
fn wire_modules_name_nothing_from_the_derived_program() {
    let root = workspace_root();
    for relative in WIRE_MODULES {
        let source = fs::read_to_string(root.join(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}"));
        for name in DERIVED_ONLY {
            assert!(
                !source.contains(name),
                "{relative} names `{name}`; the derived program is rebuilt at \
                 load, so it must not appear in a wire type"
            );
        }
    }
}

#[test]
fn the_wire_carries_no_second_copy_of_what_the_equation_ir_owns() {
    let root = workspace_root();
    let source = fs::read_to_string(root.join("crates/rumoca-ir-solve/src/execution.rs"))
        .unwrap_or_else(|error| panic!("read execution.rs: {error}"));
    // Identity only (SEV-014): a sink names a connector and its trace points,
    // and the runtime resolves paths, units and roles when it writes the
    // manifest. A `path`, `unit` or `causality` field here would be a second,
    // divergeable copy of what the equation IR already states.
    for field in ["connector_path", "pub unit", "pub causality", "pub label"] {
        assert!(
            !source.contains(field),
            "execution.rs declares `{field}`; the equation IR owns it"
        );
    }
    assert!(
        source.contains("pub trace_point: TracePointRef"),
        "a sink member must name its trace point through the id newtype: a \
         bare u32 lets a trace point, a connector and a program expression \
         index meet at a call site, and they are three different spaces"
    );
    // The same reasoning as `ProgramExprId`. A bare `u32` on either of these
    // is how the id spaces get crossed.
    for declaration in [
        "pub struct TracePointRef(pub u32)",
        "pub struct ConnectorRef(pub u32)",
        "pub connector: ConnectorRef",
    ] {
        assert!(
            source.contains(declaration),
            "execution.rs must declare `{declaration}`"
        );
    }
}

#[test]
fn no_reader_of_a_serialized_observation_list_remains() {
    let root = workspace_root();
    // D2: observations are demanded by the program. Lowering takes the trace
    // points the program references; nothing reads an `observe` list off the
    // wire or off the command line any more.
    for relative in [
        "crates/rumoca-phase-solve/src/execution.rs",
        "crates/rumoca-sim/src/execution.rs",
    ] {
        let source = fs::read_to_string(root.join(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}"));
        assert!(
            !source.contains("execution.observations") && !source.contains("\"observations\""),
            "{relative} reads an observations wire field"
        );
    }
    // Every file that could declare the flag, not just the one that used to.
    for relative in [
        "crates/rumoca/src/bitcode_cli.rs",
        "crates/rumoca/src/bitcode_execution.rs",
        "crates/rumoca/src/cli.rs",
    ] {
        let source = fs::read_to_string(root.join(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}"));
        assert!(
            !source.contains("--observe")
                && !source.contains("observe:")
                && !source.contains("pub observe"),
            "{relative} declares an observation list; lower-execution takes \
             none, because the program is the requester"
        );
    }
}
