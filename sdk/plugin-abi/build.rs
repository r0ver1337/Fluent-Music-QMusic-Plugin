fn main() {
    cc::Build::new().file("layout.c").compile("fluentAbiLayout");
    println!("cargo:rerun-if-changed=layout.c");
    println!("cargo:rerun-if-changed=include/fluentMusic");
}
