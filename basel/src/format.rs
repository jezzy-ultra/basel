use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result as AnyhowResult};
use comrak::{
    Arena as ComrakArena, ExtensionOptions as ComrakExtensionOptions,
    ListStyleType as ComrakListStyleType, Options as ComrakOptions,
    ParseOptions as ComrakParseOptions, RenderOptions as ComrakRenderOptions,
};
use json5format::{FormatOptions as Json5Options, Json5Format, ParsedDocument as Json5Parsed};
use log::{debug, info};
use quick_xml::events::Event as XmlEvent;
use quick_xml::{Reader as XmlReader, Writer as XmlWriter};
use taplo::formatter::Options as TaploOptions;

use crate::{Error, Result, is_json, is_json5, is_jsonc, is_markdown, is_svg, is_toml, is_xml};

fn toml_format_options() -> TaploOptions {
    TaploOptions {
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

fn format_toml(path: &Path) -> AnyhowResult<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let options = toml_format_options();
    let mut formatted = taplo::formatter::format(&content, options);

    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }

    if formatted == content {
        debug!("formatting unnecessary for `{}`", path.display());
    } else {
        fs::write(path, formatted)
            .with_context(|| format!("writing formatted file `{}`", path.display()))?;

        info!("formatted `{}`", path.display());
    }

    Ok(())
}

fn markdown_format_options<'a>() -> ComrakOptions<'a> {
    ComrakOptions {
        extension: ComrakExtensionOptions {
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
        parse: ComrakParseOptions {
            smart: true,
            default_info_string: None,
            relaxed_tasklist_matching: true,
            relaxed_autolinks: true,
            broken_link_callback: None,
        },
        render: ComrakRenderOptions {
            hardbreaks: false,
            github_pre_lang: true,
            full_info_string: true,
            width: 80,
            unsafe_: true,
            escape: true,
            sourcepos: false,
            list_style: ComrakListStyleType::Dash,
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

fn format_markdown(path: &Path) -> AnyhowResult<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let arena = ComrakArena::new();

    let options = markdown_format_options();

    let root = comrak::parse_document(&arena, &content, &options);

    let mut formatted = String::new();
    comrak::format_commonmark(root, &options, &mut formatted)
        .with_context(|| format!("formatting markdown file `{}`", path.display()))?;

    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }

    if formatted == content {
        debug!("formatting unnecessary for `{}`", path.display());
    } else {
        fs::write(path, formatted)
            .with_context(|| format!("writing formatted file `{}`", path.display()))?;

        info!("formatted `{}`", path.display());
    }

    Ok(())
}

enum JsonType {
    Json,
    Jsonc,
    Json5,
}

fn json_format_options() -> Json5Options {
    Json5Options {
        indent_by: 2,
        trailing_commas: false,
        collapse_containers_of_one: true,
        sort_array_items: false,
        options_by_path: HashMap::new(),
    }
}

fn jsonc_json5_format_options() -> Json5Options {
    Json5Options {
        trailing_commas: true,
        ..json_format_options()
    }
}

fn format_json(path: &Path, json_type: &JsonType) -> AnyhowResult<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let options = match json_type {
        JsonType::Json => json_format_options(),
        JsonType::Jsonc | JsonType::Json5 => jsonc_json5_format_options(),
    };

    let format = Json5Format::with_options(options)
        .with_context(|| format!("creating json5 formatter for `{}`", path.display()))?;

    let parsed = Json5Parsed::from_str(&content, Some(path.display().to_string()))
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
    } else {
        fs::write(path, formatted)
            .with_context(|| format!("writing formatted file `{}`", path.display()))?;

        info!("formatted `{}`", path.display());
    }

    Ok(())
}

const fn xml_format_options() -> (u8, usize) {
    (b' ', 2)
}

fn format_xml(path: &Path) -> AnyhowResult<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let (indent_char, indent_size) = xml_format_options();

    let mut reader = XmlReader::from_str(&content);
    reader.config_mut().check_comments = true;
    reader.config_mut().enable_all_checks(true);

    let mut writer = XmlWriter::new_with_indent(Vec::<u8>::new(), indent_char, indent_size);

    loop {
        match reader.read_event() {
            Ok(XmlEvent::Eof) => break,
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
    } else {
        fs::write(path, formatted)
            .with_context(|| format!("writing formatted file `{}`", path.display()))?;

        info!("formatted `{}`", path.display());
    }

    Ok(())
}

fn format_internal(path: &Path) -> AnyhowResult<()> {
    if is_toml(path) {
        format_toml(path)?;
    }

    if is_markdown(path) {
        format_markdown(path)?;
    }

    if is_json(path) {
        format_json(path, &JsonType::Json)?;
    }

    if is_jsonc(path) {
        format_json(path, &JsonType::Jsonc)?;
    }

    if is_json5(path) {
        format_json(path, &JsonType::Json5)?;
    }

    if is_xml(path) || is_svg(path) {
        format_xml(path)?;
    }

    Ok(())
}

pub(crate) fn format(path: &Path) -> Result<()> {
    format_internal(path).map_err(Error::formatting)
}
