fn main() {
    // The unoptimized command dispatcher and resource verification need more
    // than MSVC's 1 MiB default. Match the usual Linux main-thread stack; the
    // NWScript compiler already runs on its own explicitly sized worker stack.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=mg=/STACK:8388608");
    }
}
