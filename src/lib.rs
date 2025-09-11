pub mod scheme;

#[derive(Debug)]
pub struct Config {
    pub scheme_dir: String,
    pub template_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            scheme_dir: "schemes".to_string(),
            template_dir: "templates".to_string(),
        }
    }
}
