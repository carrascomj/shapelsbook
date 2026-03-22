pub fn app_base_path() -> &'static str {
    if cfg!(debug_assertions) {
        ""
    } else {
        "/shapelsbook"
    }
}
