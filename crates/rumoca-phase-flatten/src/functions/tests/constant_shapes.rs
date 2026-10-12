use super::*;

#[test]
fn test_convert_component_to_param_preserves_constant_shape_identity() {
    let package_def = rumoca_core::DefId::new(10);
    let n_state_def = rumoca_core::DefId::new(11);
    let mut package = class(
        "Xorshift64star",
        rumoca_core::ClassType::Package,
        package_def,
    );
    package.components.insert(
        "nState".to_string(),
        ast::Component {
            def_id: Some(n_state_def),
            type_name: ast::Name::from_string("Integer"),
            variability: rumoca_core::Variability::Constant(token("constant")),
            // Only a final declaration fixes the extent; a package extending
            // this one could otherwise modify it (MLS 3.7 §7.2).
            is_final: true,
            has_explicit_binding: true,
            binding: Some(ast::Expression::Terminal {
                terminal_type: rumoca_ir_ast::TerminalType::UnsignedInteger,
                token: token("2"),
                span: test_span(),
            }),
            ..ast::Component::empty_with_span(test_span())
        },
    );

    let mut tree = ast::ClassTree::new();
    crate::test_support::install_predefined_type_identities(&mut tree);
    tree.definitions
        .classes
        .insert("Xorshift64star".to_string(), package);
    let class_index = ast::ClassDefIndex::from_tree(&tree);
    let type_overlay = crate::test_support::type_overlay(&tree);
    let n_state_ref = ast_comp_ref(&["nState"], n_state_def);
    let mut component = ast::Component {
        type_name: ast::Name::from_string("Integer"),
        location: test_location(9, 15),
        shape_expr: vec![ast::Subscript::Expression(
            ast::Expression::ComponentReference(n_state_ref),
        )],
        ..ast::Component::empty_with_span(test_span())
    };
    component.type_name.def_id = tree
        .scope_tree
        .predefined_member(&rumoca_core::ComponentPath::from_flat_path("Integer"));
    component.type_def_id = component.type_name.def_id;
    let param = convert_component_to_param(
        &class_index,
        "state",
        &component,
        &test_source_map(),
        FunctionExpressionContext {
            predefined_intrinsics: ast_lower::PredefinedIntrinsicIds::from_tree(&tree),
            type_catalog: FunctionTypeCatalog::new(&type_overlay),
        },
        &qualify::ImportMap::default(),
        &HashSet::new(),
    )
    .unwrap();

    // A final constant fixes the extent across every callable exposure.
    assert_eq!(param.dimensions(), [2]);
    assert!(
        matches!(
            param.shape_expr.as_slice(),
            [rumoca_core::Subscript::Index { value: 2, .. }]
        ),
        "final extent must materialize as 2: {:?}",
        param.shape_expr
    );
}
