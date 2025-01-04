mod s0_lexer;
mod s1_parser;
mod s2_analyzer;
mod s3_optimizer;
mod s4_generator;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about = "Rumoca Modelica Translator", long_about = None)]
struct Args {
    /// The template
    #[arg(short, long)]
    template_file: String,

    /// The filename to compile
    #[arg(short, long)]
    filename: String,

    /// Verbose output
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let def = s1_parser::parse_file(&args.filename).expect("failed to parse");
    let flat_def = s2_analyzer::flatten(&def).expect("failed to flatten");

    if args.verbose {
        println!("{:#?}", flat_def);
    }
    let s = s4_generator::generate(&flat_def, &args.template_file)?;
    println!("{s:}");
    Ok(())
}
