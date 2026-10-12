use super::*;
use nalgebra::DMatrix;

#[test]
fn diagnostics_are_opt_in_thread_local_bounded_and_reset() {
    event(0.0);
    let scope = Scope::start().unwrap();
    assert!(Scope::start().is_err());
    std::thread::spawn(|| event(99.0)).join().unwrap();
    for i in 0..LIMIT + 3 {
        accepted_proposal(i as f64, i as f64 + 1.0);
    }
    let evidence = scope.finish();
    assert_eq!(evidence["records"].as_array().unwrap().len(), LIMIT);
    assert_eq!(evidence["omitted"], 3);
    assert_eq!(Scope::start().unwrap().finish()["records"], json!([]));
}

#[test]
fn projection_keeps_native_matrix_and_omits_oversized_values_explicitly() {
    let scope = Scope::start().unwrap();
    let matrix = BlockJacobian::dense(DMatrix::from_row_slice(2, 2, &[1., 2., 3., 4.]));
    projection("initialization", 0.0, &[1, 3], &[2, 4], &[5., 6.], &matrix);
    projection(
        "projection",
        1.0,
        &[0; 65],
        &[1],
        &[0.; 65],
        &BlockJacobian::zeros(65, 1),
    );
    let evidence = scope.finish();
    assert_eq!(
        evidence["records"][0]["jacobian_column_major"],
        json!([1., 3., 2., 4.])
    );
    assert_eq!(evidence["records"][1]["values_omitted"], true);
    assert!(evidence["records"][1]["residual"].is_null());
}
