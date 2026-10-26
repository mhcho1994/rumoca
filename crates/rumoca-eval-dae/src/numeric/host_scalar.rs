//! Exact conversion of numerical host overrides.

use super::*;

pub(super) fn no_override(_variable: dae::VariableView<'_>, _scalar: usize) -> Option<f64> {
    None
}

pub(super) fn host_scalar(
    variable: dae::VariableView<'_>,
    value: f64,
) -> Result<Scalar, NumericEvaluationError> {
    let payload = match variable.value_type().scalar_type() {
        dae::ScalarType::Real => Some(Scalar::Real(value)),
        dae::ScalarType::Integer | dae::ScalarType::Enumeration
            if value.is_finite()
                && value.fract() == 0.0
                && value.abs() <= 9_007_199_254_740_992.0 =>
        {
            Some(
                if variable.value_type().scalar_type() == dae::ScalarType::Integer {
                    Scalar::Integer(value as i64)
                } else {
                    Scalar::Enumeration(value as i64)
                },
            )
        }
        dae::ScalarType::Boolean if value == 0.0 || value == 1.0 => {
            Some(Scalar::Boolean(value == 1.0))
        }
        _ => None,
    };
    payload.ok_or_else(|| {
        failure(
            NumericEvaluationErrorKind::InvalidOverride,
            "numerical host override cannot represent the declared exact scalar type",
            variable.declaration().span(),
        )
    })
}
