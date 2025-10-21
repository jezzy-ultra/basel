use std::path::Path;

pub trait PathExt {
    fn has_extension(&self, ext: &str) -> bool;
    fn is_toml(&self) -> bool;
    fn is_jinja(&self) -> bool;
}

impl PathExt for Path {
    fn has_extension(&self, ext: &str) -> bool {
        self.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    }

    fn is_toml(&self) -> bool {
        self.has_extension("toml")
    }

    fn is_jinja(&self) -> bool {
        self.has_extension("jinja")
    }
}
