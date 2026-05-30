fn main() {
    #[cfg(target_os = "windows")]
    {
        // libgit2-sys on MSVC may require explicit Windows system libraries.
        println!("cargo:rustc-link-lib=advapi32");
    }
}
