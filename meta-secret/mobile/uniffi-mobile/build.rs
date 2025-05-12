fn main() {
    println!("cargo:rerun-if-changed=src/meta_core.udl");
    
    println!("cargo:warning=OUT_DIR: {:?}", std::env::var("OUT_DIR"));
    
    let udl = "src/meta_core.udl";
    uniffi::generate_scaffolding(udl).unwrap();
}