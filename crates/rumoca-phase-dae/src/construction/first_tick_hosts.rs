use super::*;

/// The clock partitions that read `firstTick()`, with one occurrence span each.
pub(super) fn first_tick_hosts(flat: &flat::Model, analysis: &Analysis) -> Vec<(ClockPlan, Span)> {
    let mut hosts = Vec::new();
    for (row, equation) in flat.equations.iter().enumerate() {
        if let (Some(span), Some(plan)) = (
            first_tick_span(&equation.residual),
            analysis.clocked_equation_owners.get(&row),
        ) {
            hosts.push((*plan, span));
        }
    }
    for (chain_index, chain) in flat.when_chains.iter().enumerate() {
        for (branch_index, branch) in chain.branches().enumerate() {
            let Some(span) = branch.equations.iter().find_map(when_equation_first_tick) else {
                continue;
            };
            let key = WhenBranchKey {
                chain: chain_index,
                branch: branch_index,
            };
            let named = match &branch.condition {
                Expression::VarRef { name, .. } => flat
                    .variables
                    .get(name.var_name())
                    .and_then(|variable| analysis.clock_plans.get(&variable.instance_id)),
                _ => None,
            };
            if let Some(plan) = analysis.clocked_when_owners.get(&key).or(named) {
                hosts.push((*plan, span));
            }
        }
    }
    hosts
}

fn first_tick_span(expression: &Expression) -> Option<Span> {
    if let Expression::BuiltinCall {
        function: BuiltinFunction::FirstTick,
        span,
        ..
    } = expression
    {
        return Some(*span);
    }
    expression_children(expression)
        .into_iter()
        .find_map(first_tick_span)
}

fn when_equation_first_tick(equation: &flat::WhenEquation) -> Option<Span> {
    match equation {
        flat::WhenEquation::Assign { value, .. } => first_tick_span(value),
        flat::WhenEquation::Conditional {
            branches,
            else_branch,
            ..
        } => branches
            .iter()
            .find_map(|(condition, equations)| {
                first_tick_span(condition)
                    .or_else(|| equations.iter().find_map(when_equation_first_tick))
            })
            .or_else(|| {
                else_branch
                    .iter()
                    .flatten()
                    .find_map(when_equation_first_tick)
            }),
        _ => None,
    }
}
