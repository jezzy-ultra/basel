use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr as _;

use anyhow::Context as _;
use json5format::Json5Format;
use log::{debug, info};
use strum::EnumString;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
#[strum(serialize_all = "lowercase")]
enum FileType {
    Json,
    Jsonc,
    Json5,
    Md,
    Svg,
    Toml,
    Xml,
}

impl FileType {
    fn from(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;

        Self::from_str(ext).ok()
    }
}

pub(crate) fn format(path: &Path) -> anyhow::Result<bool> {
    let Some(supported_type) = FileType::from(path) else {
        return Ok(false);
    };

    match supported_type {
        FileType::Json => json(path, json_format_options()),
        FileType::Jsonc | FileType::Json5 => json(path, jsonc_json5_format_options()),
        FileType::Md => markdown(path),
        FileType::Toml => toml(path),
        FileType::Xml | FileType::Svg => xml(path),
    }
}

fn toml(path: &Path) -> anyhow::Result<bool> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let options = toml_options();
    let mut formatted = taplo::formatter::format(&content, options);

    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }

    if formatted == content {
        debug!("formatting unnecessary for `{}`", path.display());

        Ok(false)
    } else {
        fs::write(path, formatted)
            .with_context(|| format!("writing formatted file `{}`", path.display()))?;

        info!("formatted `{}`", path.display());

        Ok(true)
    }
}

fn toml_options() -> taplo::formatter::Options {
    taplo::formatter::Options {
        align_entries: false,
        align_comments: true,
        align_single_comments: true,
        array_trailing_comma: true,
        array_auto_expand: true,
        inline_table_expand: true,
        array_auto_collapse: true,
        compact_arrays: true,
        compact_inline_tables: false,
        compact_entries: false,
        indent_tables: false,
        column_width: 80,
        indent_entries: false,
        indent_string: "  ".into(),
        trailing_newline: true,
        reorder_keys: false,
        reorder_arrays: false,
        reorder_inline_tables: false,
        allowed_blank_lines: 0,
        crlf: false,
    }
}

fn markdown(path: &Path) -> anyhow::Result<bool> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let arena = comrak::Arena::new();

    let options = markdown_options();

    let root = comrak::parse_document(&arena, &content, &options);

    let mut formatted = String::new();
    comrak::format_commonmark(root, &options, &mut formatted)
        .with_context(|| format!("formatting markdown file `{}`", path.display()))?;

    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }

    if formatted == content {
        debug!("formatting unnecessary for `{}`", path.display());

        Ok(false)
    } else {
        fs::write(path, formatted)
            .with_context(|| format!("writing formatted file `{}`", path.display()))?;

        info!("formatted `{}`", path.display());

        Ok(true)
    }
}

fn markdown_options<'a>() -> comrak::Options<'a> {
    comrak::Options {
        extension: comrak::ExtensionOptions {
            strikethrough: true,
            tagfilter: true,
            table: true,
            autolink: true,
            tasklist: true,
            superscript: true,
            header_ids: None,
            footnotes: true,
            inline_footnotes: true,
            description_lists: true,
            front_matter_delimiter: None,
            multiline_block_quotes: true,
            alerts: true,
            math_dollars: false,
            math_code: true,
            shortcodes: true,
            wikilinks_title_after_pipe: true,
            // `wikilinks_title_after_pipe` takes precedence
            wikilinks_title_before_pipe: true,
            underline: true,
            subscript: true,
            spoiler: true,
            greentext: true,
            image_url_rewriter: None,
            link_url_rewriter: None,
            cjk_friendly_emphasis: true,
        },
        parse: comrak::ParseOptions {
            smart: true,
            default_info_string: None,
            relaxed_tasklist_matching: true,
            relaxed_autolinks: true,
            broken_link_callback: None,
        },
        render: comrak::RenderOptions {
            hardbreaks: false,
            github_pre_lang: true,
            full_info_string: true,
            width: 80,
            unsafe_: true,
            escape: true,
            sourcepos: false,
            list_style: comrak::ListStyleType::Dash,
            escaped_char_spans: true,
            ignore_setext: true,
            ignore_empty_links: true,
            gfm_quirks: false,
            prefer_fenced: true,
            figure_with_caption: true,
            tasklist_classes: true,
            ol_width: 1,
            experimental_minimize_commonmark: true,
        },
    }
}

fn json_format_options() -> json5format::FormatOptions {
    json5format::FormatOptions {
        indent_by: 2,
        trailing_commas: false,
        collapse_containers_of_one: true,
        sort_array_items: false,
        options_by_path: HashMap::new(),
    }
}

fn jsonc_json5_format_options() -> json5format::FormatOptions {
    json5format::FormatOptions {
        trailing_commas: true,
        ..json_format_options()
    }
}

fn json(path: &Path, options: json5format::FormatOptions) -> anyhow::Result<bool> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let format = Json5Format::with_options(options)
        .with_context(|| format!("creating json5 formatter for `{}`", path.display()))?;

    let parsed = json5format::ParsedDocument::from_str(&content, Some(path.display().to_string()))
        .with_context(|| format!("parsing json file `{}`", path.display()))?;

    let formatted_bytes = format
        .to_utf8(&parsed)
        .with_context(|| format!("formatting json file `{}`", path.display()))?;

    let mut formatted = String::from_utf8(formatted_bytes).with_context(|| {
        format!(
            "converting formatted json to utf-8 for `{}`",
            path.display()
        )
    })?;

    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }

    if formatted == content {
        debug!("formatting unnecessary for `{}`", path.display());

        Ok(false)
    } else {
        fs::write(path, formatted)
            .with_context(|| format!("writing formatted file `{}`", path.display()))?;

        info!("formatted `{}`", path.display());

        Ok(true)
    }
}

fn xml(path: &Path) -> anyhow::Result<bool> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let (indent_char, indent_size) = xml_format_options();

    let mut reader = quick_xml::Reader::from_str(&content);
    reader.config_mut().check_comments = true;
    reader.config_mut().enable_all_checks(true);

    let mut writer = quick_xml::Writer::new_with_indent(Vec::<u8>::new(), indent_char, indent_size);

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(event) => {
                writer
                    .write_event(event)
                    .with_context(|| format!("writing xml event for `{}`", path.display()))?;
            }
            Err(e) => {
                return Err(e).with_context(|| format!("parsing xml file `{}`", path.display()));
            }
        }
    }

    let mut formatted = String::from_utf8(writer.into_inner())
        .with_context(|| format!("converting formatted xml to utf-8 for `{}`", path.display()))?;

    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }

    if formatted == content {
        debug!("formatting unnecessary for `{}`", path.display());

        Ok(false)
    } else {
        fs::write(path, formatted)
            .with_context(|| format!("writing formatted file `{}`", path.display()))?;

        info!("formatted `{}`", path.display());

        Ok(true)
    }
}

const fn xml_format_options() -> (u8, usize) {
    (b' ', 2)
}
