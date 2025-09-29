fn main() {
    build_strapped();
    build_vrf();
}

fn build_strapped() {
    const PATH: &str = "strapped/";
    // run forc build command
    let output = std::process::Command::new("forc")
        .arg("build")
        .current_dir(PATH)
        .output()
        .expect("failed to execute process");
    if !output.status.success() {
        panic!(
            "forc build failed with status: {}\nstderr: {}\n",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn build_vrf() {
    const PATH: &str = "fake-vrf-contract/";
    // run forc build command
    let output = std::process::Command::new("forc")
        .arg("build")
        .current_dir(PATH)
        .output()
        .expect("failed to execute process");
    if !output.status.success() {
        panic!(
            "forc build failed with status: {}\nstderr: {}\n",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
