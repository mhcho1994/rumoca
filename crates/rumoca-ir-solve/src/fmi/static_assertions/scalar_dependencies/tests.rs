use super::*;

#[test]
fn an_unsettled_continuation_keeps_a_fold_unsettled() {
    let domain = rumoca_core::StructuredIndexDomain {
        binders: vec![rumoca_core::StructuredIndexBinder {
            id: 0,
            display_name: "i".into(),
            lower: 1,
            upper: 2,
            step: 1,
        }],
    };
    let program = crate::FunctionFoldProgram::checked(
        domain,
        1,
        1,
        vec![Op::Const { dst: 0, value: 1.0 }, Op::StoreOutput { src: 0 }],
    )
    .unwrap()
    .with_continuation(vec![
        Op::LoadFoldCapture { dst: 0, index: 0 },
        Op::StoreOutput { src: 0 },
    ])
    .unwrap();
    let table = SolvePureCallTable::default();
    for settled in [false, true] {
        assert_eq!(
            fold_frame_outputs(&table, &program, vec![true], &[settled], &[], &[]),
            Some(vec![settled])
        );
    }
}

#[test]
fn sliced_fold_updates_read_every_index_and_value() {
    let operation = Op::StoreOutputFoldTensorUpdate {
        source_base: 0,
        source_stride: 1,
        dimensions: Box::new([3]),
        lanes: 1,
        updates: vec![crate::FoldTensorUpdate {
            subscripts: vec![crate::TensorUpdateSubscript::Slice {
                start: 0,
                dimensions: Box::new([2]),
            }]
            .into_boxed_slice(),
            condition: None,
            value_start: 2,
            value_stride: 1,
        }]
        .into_boxed_slice(),
        nodes: vec![crate::FoldTensorNode::Update { base: 0, update: 0 }].into_boxed_slice(),
        result: 1,
    };
    let frame = Frame {
        carried: &[true; 3],
        ..Frame::default()
    };
    assert_eq!(
        tensor_update_outputs(&operation, &[true; 4], frame),
        Some(vec![true; 3])
    );
    for position in 0..4 {
        let mut registers = [true; 4];
        registers[position] = false;
        assert_eq!(
            tensor_update_outputs(&operation, &registers, frame),
            Some(vec![false; 3])
        );
    }
}
