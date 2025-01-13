use super::dae_ast;
use rumoca_parser::ast::{Equation, Expression};
use rumoca_parser::Visitor;
use rumoca_parser::{ast, Visitable};
use std::collections::HashMap;

//=============================================================================
/// Builds a dae_ast from a flattened rumoca_parser::ast
pub fn create_dae(
    def: &ast::StoredDefinition,
    verbose: bool,
) -> Result<dae_ast::Def, Box<dyn std::error::Error>> {
    if verbose {
        println!("\n\n{}", "=".repeat(80));
        println!("CREATE DAE");
        println!("{}", "=".repeat(80));
    }

    let mut component_finder = ComponentFinder::default();
    def.accept(&mut component_finder);

    if verbose {
        println!("\n\nstep 1 (find components)\n=========================\n");
        println!("{:#?}", component_finder);
    }

    let mut state_finder = StateFinder {
        component_finder,
        ..Default::default()
    };
    def.accept(&mut state_finder);

    if verbose {
        println!("\n\nstep 2 (find states)\n=========================\n");
        println!("{:#?}", state_finder);
    }

    let mut dae_creator = DaeCreator {
        state_finder,
        ..Default::default()
    };

    def.accept(&mut dae_creator);

    if verbose {
        println!("\n\nstep 3 (build dae ast)\n=========================\n");
        println!("{:#?}", dae_creator.def);
    }

    Ok(dae_creator.def)
}

//=============================================================================
/// Builds a dae_ast from a flattened rumoca_parser::ast
#[derive(Default, Debug)]
pub struct DaeCreator<'a> {
    def: dae_ast::Def,
    #[allow(dead_code)]
    state_finder: StateFinder<'a>,
}

impl<'a> Visitor<'a> for DaeCreator<'a> {
    fn enter_stored_definition(&mut self, node: &'a ast::StoredDefinition) {
        self.def.rumoca_parser_git = node.rumoca_parser_git.clone();
        self.def.rumoca_parser_version = node.rumoca_parser_version.clone();
        self.def.rumoca_version = env!("CARGO_PKG_VERSION").to_string();
        self.def.rumoca_git = option_env!("GIT_VER").unwrap_or("").to_string();
    }

    fn enter_class_definition(&mut self, node: &'a ast::ClassDefinition) {
        let mut class = dae_ast::Class {
            ..Default::default()
        };
        if let ast::ClassSpecifier::Long { name, .. } = &node.specifier {
            for x in self.state_finder.states.keys() {
                class.name = name.clone();
                class.x.insert(x.clone());
                class.ode.insert(
                    x.clone(),
                    (*self.state_finder.ode.get(x).expect("failed to find ode")).clone(),
                );
            }
            self.def.classes.insert(name.clone(), class);
        }
    }
}

//=============================================================================
/// Finds all states in the class
#[derive(Default, Debug)]
pub struct StateFinder<'a> {
    /// A struct to traverse a tree and find all calsses and put references
    /// in a dictionary. The references have the same lifetime as the
    /// struct.s
    states: HashMap<String, &'a ast::ComponentDeclaration>,
    ode: HashMap<String, &'a ast::Expression>,

    // from component namer
    component_finder: ComponentFinder<'a>,
}

impl<'a> Visitor<'a> for StateFinder<'a> {
    fn enter_equation(&mut self, node: &'a Equation) {
        // looks for simple explicit ode of form der(x) = expr
        if let Equation::Simple {
            lhs: Expression::Der { args },
            rhs,
            ..
        } = &node
        {
            if let Expression::Ref { comp } = &args[0] {
                let comp_str = self
                    .component_finder
                    .component_ref_to_str
                    .get(&comp)
                    .expect("failed to get comp")
                    .clone();
                let comp_decl = self
                    .component_finder
                    .str_to_component
                    .get(&comp_str)
                    .expect("failed to get str");
                self.states.insert(comp_str.clone(), comp_decl);
                self.ode.insert(comp_str.clone(), rhs);
            }
        }
    }
}

//=============================================================================
/// Find all components, and expands component refs to strings
#[derive(Default, Debug)]
pub struct ComponentFinder<'a> {
    component_ref_to_str: HashMap<&'a ast::ComponentReference, String>,
    component_to_str: HashMap<&'a ast::ComponentDeclaration, String>,
    str_to_component: HashMap<String, &'a ast::ComponentDeclaration>,
    scope: Vec<String>,
}

impl<'a> Visitor<'a> for ComponentFinder<'a> {
    /// pushes class scope
    fn enter_class_definition(&mut self, node: &'a ast::ClassDefinition) {
        if let ast::ClassSpecifier::Long { name, .. } = &node.specifier {
            self.scope.push(name.clone());
        }
    }

    /// pops class scope
    fn exit_class_definition(&mut self, node: &'a ast::ClassDefinition) {
        if let ast::ClassSpecifier::Long { .. } = &node.specifier {
            self.scope.pop();
        }
    }

    /// creates lookup for component from name ane name from component
    fn enter_component_declaration(&mut self, node: &'a ast::ComponentDeclaration) {
        let s = format!("{}.{}", self.scope.join("."), node.declaration.name);
        self.component_to_str.insert(node, s.clone());
        self.str_to_component.insert(s.clone(), node);
    }

    /// expands component ref to string
    fn enter_component_reference(&mut self, node: &'a ast::ComponentReference) {
        let mut s: String = "".to_string();
        for (index, part) in node.parts.iter().enumerate() {
            if index != 0 || node.local {
                s += ".";
            }
            s += &part.name;
        }
        self.component_ref_to_str
            .insert(node, format!("{}.{}", self.scope.join("."), s));
    }
}
