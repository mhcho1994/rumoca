use crate::s2_analyzer::dae_ast as ast;
use minijinja::{context, Environment};

pub fn panic(msg: &str) {
    panic!("{:?}", msg);
}

pub fn warn(msg: &str) {
    eprintln!("{:?}", msg);
}

pub fn generate(
    def: &mut ast::Def,
    template_file: &str,
    verbose: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    if verbose {
        println!("\n\n{}", "=".repeat(80));
        println!("GENERATING");
        println!("{}", "=".repeat(80));
    }
    let template_txt = std::fs::read_to_string(template_file)?;
    let digest = md5::compute(&template_txt);
    def.template_md5 = format!("{:x}", digest);

    let mut env = Environment::new();
    env.add_function("panic", panic);
    env.add_function("warn", warn);
    env.add_template("template", &template_txt)?;
    let tmpl = env.get_template("template")?;
    let txt = tmpl.render(context!(def => def)).unwrap();
    Ok(txt)
}
