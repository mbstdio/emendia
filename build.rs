fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/emendia.rc");
    println!("cargo:rerun-if-changed=src/ressources/app-logo.ico");
    embed_resource::compile("packaging/windows/emendia.rc", embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed the Emendia icon");
}
