use rumoca_parser::ast;
use rumoca_parser::{PrintVisitor, Visitable, VisitableMut, Visitor, VisitorMut};
use std::collections::HashMap;

//=============================================================================
/// Flattens nested classes into one class (see Friztzon pg. 143)
///
/// 1. The names of declared local classes, variables, and other attributes
///     are found. Also, modifiers are merged with the local element
///     declarations, and redeclarations are applied.
///
/// 2. Extends clauses are processed by lookup and expansion of inherited
///     classes. Their contents are expanded and inserted into the current
///     class. The lookup of the inherited classes should be independent,
///     that is, the analysis and expansion of one extends clause should
///     not be dependent on another.
pub fn flatten(
    def: &ast::StoredDefinition,
    verbose: bool,
) -> Result<ast::StoredDefinition, Box<dyn std::error::Error>> {
    if verbose {
        println!("\n\n=========================");
        println!("FLATTEN");
        println!("=========================");
    }

    //
    // STEP 1 (find classes)
    //
    let mut class_collector = ClassCollector::default();
    def.accept(&mut class_collector);

    if verbose {
        println!("\n\nstep 1.1 (find classes)\n=========================\n");
        println!("{:#?}", class_collector.classes);
    }

    // 1.2 find variables

    let mut vars = HashMap::new();
    for (class_name, class_def) in class_collector.classes.iter() {
        // for the classes that we found, find all of their components
        // and store them in a dictionary
        let mut var_collector = VarCollector::default();
        class_def.accept(&mut var_collector);
        vars.insert(class_name.clone(), var_collector.vars);
    }

    if verbose {
        println!("\n\nstep 1.2 (find variables)\n=========================\n");
        println!("{:#?}", vars);
    }

    //
    // STEP 2 (process extends clauses etc.)
    //
    if verbose {
        println!("\n\nstep 2 (extends)\n=========================\n");
    }

    //
    // STEP 3 (test mutation)
    //
    let mut class_namer = ClassNamer::default();
    let mut flat_tree = def.clone();
    flat_tree.accept_mut(&mut class_namer);

    if verbose {
        println!("\n\nstep 3 (test mutation)\n=========================\n");
        let mut print_visitor = PrintVisitor::default();
        flat_tree.accept(&mut print_visitor);
    }

    //
    // Return
    //
    Ok(flat_tree)
}

//=============================================================================
/// Collects classes into a dictionary with names as keys
///
#[derive(Default)]
struct ClassCollector<'a> {
    /// A struct to traverse a tree and find all calsses and put references
    /// in a dictionary. The references have the same lifetime as the
    /// struct.
    classes: HashMap<String, &'a ast::ClassDefinition>,
}

impl<'a> Visitor<'a> for ClassCollector<'a> {
    /// Visits the parse tree, storing classes in a dictionary by name
    fn enter_class_definition(&mut self, node: &'a ast::ClassDefinition) {
        #[allow(clippy::single_match)]
        match &node.specifier {
            ast::ClassSpecifier::Long { name, .. } => {
                self.classes.insert(name.clone(), node);
            }
            _ => {}
        }
    }
}

//=============================================================================
/// Collects variables for each class
///
#[derive(Default)]
struct VarCollector<'a> {
    /// A struct to hold a dictionary of variables to component definitions
    vars: HashMap<String, &'a ast::ComponentDeclaration>,
}

impl<'a> Visitor<'a> for VarCollector<'a> {
    fn enter_component_declaration(&mut self, node: &'a ast::ComponentDeclaration) {
        self.vars.insert(node.declaration.name.clone(), node);
    }
}

/// Renames all classes "test" as a tree mutation test
#[derive(Default, Debug, Clone)]
struct ClassNamer {}

impl VisitorMut for ClassNamer {
    fn enter_class_definition_mut(&mut self, node: &mut ast::ClassDefinition) {
        #[allow(clippy::single_match)]
        match &mut node.specifier {
            ast::ClassSpecifier::Long { name, .. } => {
                *name = "test".into();
            }
            _ => {}
        }
    }
}
