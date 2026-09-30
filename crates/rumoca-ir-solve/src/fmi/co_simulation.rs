//! How a generated Co-Simulation component advances one communication step
//! (SPEC_0044 ME-LSW-001).
//!
//! The plan is constructed with the component, so every generated
//! Co-Simulation executor (the FMI 2 and 3 C profiles and fmi-ls-wasm, which
//! compiles the same C) integrates by the same rule instead of each template
//! choosing its own method and substep.

use serde::Serialize;

/// The explicit integration method of a Co-Simulation substep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CoSimulationMethod {
    /// Classical fourth-order Runge-Kutta, with the derivative refreshed at
    /// every stage.
    ClassicalRk4,
}

/// The longest substep a communication step is integrated by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CoSimulationSubstep {
    /// One substep per communication step, shortened only where a state
    /// event must be located (the component's `RootLocationPlan` scan).
    CommunicationStep,
}

/// The Co-Simulation step rule of one FMI component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CoSimulationStepPlan {
    method: CoSimulationMethod,
    substep: CoSimulationSubstep,
}

impl CoSimulationStepPlan {
    /// The plan every component is constructed with. A fixed-step method has
    /// no error control: its accuracy follows the importer's communication
    /// step, which is why this is a component fact an importer can read.
    pub const STANDARD: Self = Self {
        method: CoSimulationMethod::ClassicalRk4,
        substep: CoSimulationSubstep::CommunicationStep,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Templates select their substep integrator by these names, so the
    /// serialized plan is part of the rendering contract.
    #[test]
    fn the_standard_plan_serializes_the_names_templates_read() {
        let plan = serde_json::to_value(CoSimulationStepPlan::STANDARD).expect("serializes");
        assert_eq!(
            plan,
            serde_json::json!({"method": "ClassicalRk4", "substep": "CommunicationStep"})
        );
    }
}
