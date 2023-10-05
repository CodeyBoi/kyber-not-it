fn main() {
    println!("cargo:rustc-link-search=src/libs");
    println!("cargo:rustc-link-lib=static=frodo");
}
