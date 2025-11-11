use serde::Serialize;

#[derive(Debug, Default)]
pub(crate) struct Style {
    pub color: ColorStyle,
    pub text: TextStyle,
}

#[non_exhaustive]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum ColorStyle {
    #[default]
    Hex,
    Name,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum TextStyle {
    #[default]
    Unicode,
    Ascii,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Unicode;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Ascii;
