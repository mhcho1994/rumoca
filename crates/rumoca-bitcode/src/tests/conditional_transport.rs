//! Assigned reads must survive public transport with their checked owner.
use crate::schema::*;
use rumoca_core::{SourceMap, Span, VarName};
use rumoca_ir_dae as dae;

fn partial_output() -> RbcFile {
    let mut sources = SourceMap::new();
    let source = sources.add("partial.mo", "function choose");
    let at = dae::DaeProvenance::source(Span::from_offsets(source, 0, 15)).unwrap();
    let model = dae::Dae::construct(sources, |construction| {
        let (real, boolean) = construction.types(|types| {
            Ok((
                types.derived(dae::ValueType::scalar(dae::ScalarType::Real), at)?,
                types.derived(dae::ValueType::scalar(dae::ScalarType::Boolean), at)?,
            ))
        })?;
        construction.function(
            dae::FunctionSignature::new(VarName::new("choose"), [boolean], [real], at),
            |c, reservation| {
                let (flag, output) = c.functions(|f| {
                    Ok((
                        f.parameter(&reservation, VarName::new("flag"), 0, at)?,
                        f.output(&reservation, VarName::new("y"), 0, at)?,
                    ))
                })?;
                let (condition, one) = c.expressions(|e| {
                    Ok((
                        e.at(at).function_parameter(flag)?,
                        e.at(at).literal(dae::DaeLiteral::Real(1.0))?,
                    ))
                })?;
                let body = c.functions(|f| f.begin(reservation, at))?;
                let (mut child, finish) = c.functions(|f| f.begin_branch(body, at))?;
                c.functions(|f| f.assign(&mut child, output, one, at))?;
                let (body, branch) = c.functions(|f| f.finish_branch(finish, child))?;
                let (child, finish) = c.functions(|f| f.begin_branch(body, at))?;
                let (mut body, fallback) = c.functions(|f| f.finish_branch(finish, child))?;
                c.functions(|f| {
                    f.finish_conditional(&mut body, vec![(condition, branch)], fallback, at)?;
                    f.require_assigned(&mut body, output, at)?;
                    f.define(body, at)
                })
            },
        )?;
        Ok(())
    })
    .expect("checked partial output");
    crate::export(&model, None, "Partial", &Default::default()).unwrap()
}

#[test]
fn assigned_read_round_trip_preserves_its_ordered_action() {
    let original = partial_output();
    let dae = crate::import(&original).expect("assigned read rebuilds");
    let again = crate::export(&dae, None, "Partial", &Default::default()).unwrap();
    assert_eq!(
        serde_json::to_value(&original.model.functions).unwrap(),
        serde_json::to_value(&again.model.functions).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&original.model.expressions).unwrap(),
        serde_json::to_value(&again.model.expressions).unwrap()
    );
}

#[test]
fn assigned_read_rejects_a_forged_owner() {
    let mut artifact = partial_output();
    let output = artifact
        .model
        .expressions
        .iter_mut()
        .find(|expression| {
            matches!(
                expression.node,
                RbcExprNode::FunctionConditionalOutput { .. }
            )
        })
        .unwrap();
    if let RbcExprNode::FunctionConditionalOutput { conditional, .. } = &mut output.node {
        *conditional = u32::MAX;
    }
    assert!(crate::import(&artifact).is_err());
}
