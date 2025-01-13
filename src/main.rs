use clap::Parser;

mod s2_analyzer;
mod s3_optimizer;
mod s4_generator;

use rumoca_parser::{PrintVisitor, Visitable};

#[macro_use]
extern crate macro_rules_attribute;

#[derive(Parser, Debug)]
#[command(version, about = "Rumoca Modelica Translator", long_about = None)]
struct Args {
    /// The template
    #[arg(short, long, default_value = "")]
    template_file: String,

    /// The model file to compile
    #[arg(short, long)]
    model_file: String,

    /// Verbose output
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let def = rumoca_parser::parse_file(&args.model_file);

    if args.verbose {
        println!("\n\n{}", "=".repeat(80));
        println!("PARSE");
        println!("{}", "=".repeat(80));
        def.accept(&mut PrintVisitor::default());
    }

    let _flat_def = s2_analyzer::flatten(&def, args.verbose).expect("failed to flatten");
    let mut dae_def =
        s2_analyzer::dae_creator::create_dae(&def, args.verbose).expect("failed to create dae");

    if !args.template_file.is_empty() {
        let s = s4_generator::generate(&mut dae_def, &args.template_file, args.verbose)?;
        println!("{s:}");
    }
    Ok(())
}
