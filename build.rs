use rerun_except::rerun_except;

fn main() {
    lalrpop::process_root().unwrap();
    rerun_except(&["*.mo", "*.jinja", "*.sdf", "*.py"]).unwrap();
}
