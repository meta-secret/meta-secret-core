fn main() {
    let udl = "src/meta-core.udl";
    uniffi::generate_scaffolding(udl).unwrap();
}