fn main() {
    // Configure the SQLite build for better compatibility
    // These flags help with cross-platform builds
    println!("cargo:rustc-env=SQLITE_MAX_VARIABLE_NUMBER=500");
    println!("cargo:rustc-env=SQLITE_THREADSAFE=1");
    println!("cargo:rustc-env=SQLITE_DEFAULT_MEMSTATUS=0");
    println!("cargo:rustc-env=SQLITE_DEFAULT_WAL_SYNCHRONOUS=1");

    // On Windows, we need to be more careful
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-cfg=libsqlite3_sys");
    }
}
