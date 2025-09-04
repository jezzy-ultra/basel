use hex_color::HexColor;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

pub const THEMES_DIR: &str = "themes/";

#[derive(Deserialize, Debug)]
pub struct Theme {
    #[serde(flatten)]
    pub meta: Meta,

    pub palette: HashMap<String, HexColor>,
    pub roles: Roles,

    #[serde(flatten)]
    pub unused: HashMap<String, String>,
}

impl Theme {
    pub fn themes() -> HashMap<String, Theme> {
        let mut themes = HashMap::new();

        if let Ok(entries) = fs::read_dir(THEMES_DIR) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().unwrap() == "toml" {
                    let theme = Self::read(path.to_str().unwrap());
                    let name = path.file_name().unwrap().to_str().unwrap().to_string();
                    themes.insert(name, theme);
                }
            }
        }

        themes
    }

    fn read(path: &str) -> Theme {
        let content = fs::read_to_string(path).unwrap();
        let theme: Theme = toml::from_str(&content).unwrap();

        theme
    }
}

#[derive(Deserialize, Debug)]
pub struct Meta {
    pub author: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
#[serde(try_from = "RawRoles")]
pub struct Roles {
    background: String,
    foreground: String,
    color00: String,
    color01: String,
    color02: String,
    color03: String,
    color04: String,
    color05: String,
    color06: String,
    color07: String,
    color08: String,
    color09: String,
    color10: String,
    color11: String,
    color12: String,
    color13: String,
    color14: String,
    color15: String,
}

impl From<RawRoles> for Roles {
    fn from(raw: RawRoles) -> Self {
        let background = raw.background.clone();
        let foreground = raw.foreground.clone();
        let color00 = raw.color00.unwrap_or_else(|| background.clone());
        let color01 = raw.color01.clone();
        let color02 = raw.color02.clone();
        let color03 = raw.color03.clone();
        let color04 = raw.color04.clone();
        let color05 = raw.color05.clone();
        let color06 = raw.color06.clone();
        let color07 = raw.color07.unwrap_or_else(|| foreground.clone());
        let color08 = raw.color08.unwrap_or_else(|| color00.clone());
        let color09 = raw.color09.unwrap_or_else(|| color01.clone());
        let color10 = raw.color10.unwrap_or_else(|| color02.clone());
        let color11 = raw.color11.unwrap_or_else(|| color03.clone());
        let color12 = raw.color12.unwrap_or_else(|| color04.clone());
        let color13 = raw.color13.unwrap_or_else(|| color05.clone());
        let color14 = raw.color14.unwrap_or_else(|| color06.clone());
        let color15 = raw.color15.unwrap_or_else(|| color07.clone());

        Self {
            background,
            foreground,
            color00,
            color01,
            color02,
            color03,
            color04,
            color05,
            color06,
            color07,
            color08,
            color09,
            color10,
            color11,
            color12,
            color13,
            color14,
            color15,
        }
    }
}

#[derive(Deserialize, Debug)]
struct RawRoles {
    background: String,
    foreground: String,
    color00: Option<String>,
    color01: String,
    color02: String,
    color03: String,
    color04: String,
    color05: String,
    color06: String,
    color07: Option<String>,
    color08: Option<String>,
    color09: Option<String>,
    color10: Option<String>,
    color11: Option<String>,
    color12: Option<String>,
    color13: Option<String>,
    color14: Option<String>,
    color15: Option<String>,
}
