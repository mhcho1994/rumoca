use std::process::Command;

fn main() {
    // create git hash
    let output = Command::new("git")
        .args(["describe", "--dirty", "--tags"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
}
