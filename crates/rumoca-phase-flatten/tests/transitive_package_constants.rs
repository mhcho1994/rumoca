//! Exact-identity regression coverage for transitive package constants.
//!
//! A retained Resolve dependency closure may keep only the package/class
//! owners needed by the selected model. Flat constant materialization must
//! therefore follow the resolved root identity and structured member path,
//! rather than depending on a rendered-name search over unrelated classes.

use rumoca_core::ExpressionVisitor;
use rumoca_ir_ast as ast;

const SOURCE_NAME: &str = "<transitive_package_constants>";
const SOURCE: &str = r#"
package Services
    package Machine
        final constant Real eps = 0.125;
    end Machine;
end Services;

package Library
    package Constants
        final constant Real eps = Services.Machine.eps;
    end Constants;

    model Top
        Real y;
        Real z;
    equation
        y = Library.Constants.eps;
        z = Other.Constants.eps;
    end Top;
end Library;

package Other
    package Constants
        final constant Real eps = 0.5;
    end Constants;
end Other;
"#;

#[derive(Default)]
struct ConstantUseCollector {
    literals: Vec<(f64, rumoca_core::Span)>,
    references: Vec<String>,
}

impl ExpressionVisitor for ConstantUseCollector {
    fn visit_expression(&mut self, expression: &rumoca_core::Expression) {
        if let rumoca_core::Expression::Literal {
            value: rumoca_core::Literal::Real(value),
            span,
        } = expression
        {
            self.literals.push((*value, *span));
        }
        self.walk_expression(expression);
    }

    fn visit_var_ref(
        &mut self,
        name: &rumoca_core::Reference,
        subscripts: &[rumoca_core::Subscript],
    ) {
        self.references.push(name.as_str().to_string());
        self.walk_var_ref(name, subscripts);
    }
}

#[test]
fn same_leaf_package_constants_materialize_by_exact_target_at_each_use_site() {
    let stored = rumoca_phase_parse::parse_to_ast(SOURCE, SOURCE_NAME).expect("source parses");
    let mut tree = ast::ClassTree::from_parsed(stored);
    tree.source_map.add(SOURCE_NAME, SOURCE);
    let resolved =
        rumoca_phase_resolve::resolve(ast::ParsedTree::new(tree)).expect("source resolves");
    let instanced =
        rumoca_phase_instantiate::instantiate(resolved, "Library.Top").expect("model instantiates");
    let ast::InstancedTree { tree, mut overlay } = instanced;
    rumoca_phase_typecheck::typecheck_instanced(&tree, &mut overlay, "Library.Top")
        .expect("instanced model typechecks");
    let model =
        rumoca_phase_flatten::flatten_ref(&tree, &overlay, "Library.Top").expect("model flattens");

    let mut collector = ConstantUseCollector::default();
    for equation in &model.equations {
        collector.visit_expression(&equation.residual);
    }

    // Real package constants stay named: each use names the exact
    // declaration it resolved to, and that declaration is materialized once
    // as a model constant carrying its value (docs/design/minimal-frontend.md).
    assert!(
        collector
            .references
            .iter()
            .any(|name| name == "Library.Constants.eps")
            && collector
                .references
                .iter()
                .any(|name| name == "Other.Constants.eps"),
        "each use names its own declaration, got {:?}",
        collector.references
    );
    assert!(
        !collector
            .references
            .iter()
            .any(|name| name == "Services.Machine.eps"),
        "the transitive constant is folded into the value, not referenced: {:?}",
        collector.references
    );
    let declared = |name: &str| {
        let variable = model
            .variables
            .get(&rumoca_core::VarName::new(name))
            .unwrap_or_else(|| panic!("`{name}` is materialized"));
        assert!(
            matches!(variable.variability, rumoca_core::Variability::Constant(_)),
            "`{name}` is a constant"
        );
        match &variable.binding {
            Some(rumoca_core::Expression::Literal {
                value: rumoca_core::Literal::Real(value),
                ..
            }) => *value,
            other => panic!("`{name}` is bound to its value, got {other:?}"),
        }
    };
    assert_eq!(
        declared("Library.Constants.eps"),
        0.125,
        "resolved through Services.Machine.eps"
    );
    assert_eq!(
        declared("Other.Constants.eps"),
        0.5,
        "same-named leaves in another package cannot cross-bind"
    );
}
