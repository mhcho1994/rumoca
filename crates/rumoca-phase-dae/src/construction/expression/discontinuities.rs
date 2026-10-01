//! MLS §3.7.2 event-generating builtins at model scope.

use super::*;

/// Lower an event-generating builtin through its checked runtime owner, or
/// return `None` when the plain pure builtin is exact.
///
/// `div`/`mod`/`rem` (MLS §3.7.2) generate events where the quotient
/// changes: the static path folds a fully static call; a call the static
/// proof refuses as a runtime discontinuity is handed to the checked runtime
/// owner, which admits it exactly when the divisor is proven time-invariant
/// and builds the sin-indicator event root. A statically undefined domain
/// (proven zero divisor) stays rejected — only the non-static refusal
/// reroutes.
///
/// `floor`/`ceil`/`integer` generate events where their value changes. With a
/// continuous-time scalar argument they are lowered through the same owner
/// (see [`lower_event_rounding`]); without it a discrete `Integer` defined as
/// `integer(floor(time / T))` was never re-evaluated, because nothing
/// triggered the event at which discrete values update.
pub(super) fn lower_event_discontinuity<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    symbols: LoweringSymbols<'_, 'dae>,
    builtin: dae::PureBuiltin,
    arguments: &[dae::ExprId<'dae>],
    provenance: dae::DaeProvenance,
) -> Result<Option<dae::ExprId<'dae>>, dae::DaeConstructionError> {
    match (builtin, arguments) {
        (dae::PureBuiltin::Div | dae::PureBuiltin::Mod | dae::PureBuiltin::Rem, [lhs, rhs]) => {
            let (lhs, rhs) = (*lhs, *rhs);
            let attempted = construction.expressions(|expressions| {
                expressions
                    .at(provenance)
                    .builtin(builtin, arguments.iter().copied())
            });
            let Err(dae::DaeConstructionError::NonStaticDiscontinuity { .. }) = attempted else {
                return attempted.map(Some);
            };
            // MLS §3.7.2: no events are generated inside a function body — the
            // quotient stands alone, proven against the exact open body
            // capability; at model scope the checked runtime owner builds the
            // discontinuity root.
            if let Some(body) = symbols.function_body {
                return construction
                    .function_runtime_quotient(body, builtin, [lhs, rhs], provenance)
                    .map(Some);
            }
            construction
                .runtime_quotient(builtin, [lhs, rhs], provenance)
                .map(Some)
        }
        (dae::PureBuiltin::Floor | dae::PureBuiltin::Ceil | dae::PureBuiltin::Integer, [x]) => {
            lower_event_rounding(construction, symbols, builtin, *x, provenance)
        }
        _ => Ok(None),
    }
}

/// `floor`, `ceil` and `integer` of a continuous-time scalar, with the events
/// MLS §3.7.2 requires where the value changes.
///
/// The event surface is the checked runtime-quotient owner of `div(x, 1)`,
/// whose indicator `sin(pi * x)` vanishes exactly at the integers, where
/// every one of these functions jumps. The value is exact in binary64:
/// `div(x, 1)` truncates toward zero, so `floor(x) = div(x, 1) - 1` when
/// `x < div(x, 1)` (a negative non-integer) and `div(x, 1)` otherwise;
/// `ceil(x) = 0 - floor(-x)`; `integer(x)` is the pure builtin applied to that
/// already-integral `floor(x)`. Inside a function body (MLS §3.7.2: no events),
/// in a clocked partition (values change only at clock ticks), or for a
/// constant, parameter, discrete or array argument, the plain builtin is
/// already exact and is returned unchanged (`None`).
fn lower_event_rounding<'dae>(
    construction: &mut dae::DaeConstruction<'dae>,
    symbols: LoweringSymbols<'_, 'dae>,
    builtin: dae::PureBuiltin,
    x: dae::ExprId<'dae>,
    provenance: dae::DaeProvenance,
) -> Result<Option<dae::ExprId<'dae>>, dae::DaeConstructionError> {
    if symbols.function_body.is_some() || symbols.owner_clock.is_some() {
        return Ok(None);
    }
    let (variability, value_type) = construction.expressions(|expressions| {
        Ok((
            expressions.variability(x, provenance)?,
            expressions.value_type(x, provenance)?,
        ))
    })?;
    if variability != dae::ExpressionVariability::Continuous || !value_type.is_scalar() {
        return Ok(None);
    }
    let generated =
        dae::DaeProvenance::generated(dae::DaeGeneration::RuntimeDiscontinuity, provenance.span())?;
    let argument = if builtin == dae::PureBuiltin::Ceil {
        construction.expressions(|expressions| {
            expressions
                .at(generated)
                .unary(dae::UnaryOperator::Negate, x)
        })?
    } else {
        x
    };
    let one = construction.expressions(|expressions| {
        expressions
            .at(generated)
            .literal(dae::DaeLiteral::Real(1.0))
    })?;
    let truncated =
        construction.runtime_quotient(dae::PureBuiltin::Div, [argument, one], provenance)?;
    let floored = construction.expressions(|expressions| {
        let below =
            expressions
                .at(generated)
                .binary(dae::BinaryOperator::Less, argument, truncated)?;
        let lowered =
            expressions
                .at(generated)
                .binary(dae::BinaryOperator::Subtract, truncated, one)?;
        expressions
            .at(generated)
            .conditional([(below, lowered)], truncated)
    })?;
    let value = construction.expressions(|expressions| match builtin {
        dae::PureBuiltin::Floor => Ok(floored),
        // `0 - floor(-x)`, not `-floor(-x)`: an integral zero stays `+0`.
        dae::PureBuiltin::Ceil => {
            let zero = expressions
                .at(generated)
                .literal(dae::DaeLiteral::Real(0.0))?;
            expressions
                .at(provenance)
                .binary(dae::BinaryOperator::Subtract, zero, floored)
        }
        _ => expressions.at(provenance).builtin(builtin, [floored]),
    })?;
    Ok(Some(value))
}
