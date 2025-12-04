use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr as _;

use anyhow::{Context as _, anyhow};
use json5format::Json5Format;
use log::{debug, info};
use strum::EnumString;
use tombi_config::{FormatOptions as TombiFormatOptions, TomlVersion as TombiTomlVersion};
use tombi_formatter::Formatter as TombiFormatter;
use tombi_schema_store::SchemaStore as TombiSchemaStore;

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

pub(crate) async fn format(content: &str, path: &Path) -> anyhow::Result<Option<String>> {
    let Some(supported_type) = FileType::from(path) else {
        return Ok(None);
    };

    let formatted = match supported_type {
        FileType::Json => json(content, json_format_options()),
        FileType::Jsonc | FileType::Json5 => json(content, jsonc_json5_format_options()),
        FileType::Md => markdown(content),
        FileType::Toml => toml(content).await,
        FileType::Xml | FileType::Svg => xml(content),
    };

    if formatted == content {
        Ok(None)
    } else {
        Ok(Some(formatted))
    }
}

async fn toml(path: &Path) -> anyhow::Result<bool> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    // FIXME: actually handle schemas
    let schema_store = TombiSchemaStore::new();

    let formatted = TombiFormatter::new(
        TombiTomlVersion::default(),
        &TombiFormatOptions::default(),
        None,
        &schema_store,
    )
    .format(&content)
    .await
    .map_err(|src| anyhow!("formatting toml file `{}`: {src:?}", path.display()))?;

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

fn markdown(path: &Path) -> anyhow::Result<bool> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let arena = comrak::Arena::new();

    let options = markdown_options();

    let root = comrak::parse_document(&arena, &content, &options);

    let mut formatted = String::new();
    comrak::format_commonmark(root, &options, &mut formatted)
        .with_context(|| format!("formatting markdown file `{}`", path.display()))?;

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
    use comrak::options::{Extension, ListStyleType, Parse, Render};

    comrak::Options {
        extension: Extension {
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
            subtext: true,
            highlight: true,
        },
        parse: Parse {
            smart: true,
            default_info_string: None,
            relaxed_tasklist_matching: true,
            tasklist_in_table: true,
            relaxed_autolinks: true,
            broken_link_callback: None,
            ignore_setext: true,
            leave_footnote_definitions: false,
            escaped_char_spans: true,
        },
        render: Render {
            hardbreaks: false,
            github_pre_lang: true,
            full_info_string: true,
            width: 80,
            r#unsafe: true,
            escape: true,
            sourcepos: false,
            list_style: ListStyleType::Dash,
            escaped_char_spans: true,
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

fn json(content: &str, options: json5format::FormatOptions) -> anyhow::Result<bool> {
    let format = Json5Format::with_options(options)
        .with_context(|| format!("creating json5 formatter for `{}`", content.display()))?;

    let parsed =
        json5format::ParsedDocument::from_str(&content, Some(content.display().to_string()))
            .with_context(|| format!("parsing json file `{}`", content.display()))?;

    let formatted_bytes = format
        .to_utf8(&parsed)
        .with_context(|| format!("formatting json file `{}`", content.display()))?;

    let mut formatted = String::from_utf8(formatted_bytes).with_context(|| {
        format!(
            "converting formatted json to utf-8 for `{}`",
            content.display()
        )
    })?;

    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }

    if formatted == content {
        debug!("formatting unnecessary for `{}`", content.display());

        Ok(false)
    } else {
        fs::write(content, formatted)
            .with_context(|| format!("writing formatted file `{}`", content.display()))?;

        info!("formatted `{}`", content.display());

        Ok(true)
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

fn xml(path: &Path) -> anyhow::Result<bool> {
    use quick_xml::events::Event;

    let content = fs::read_to_string(path)
        .with_context(|| format!("reading file `{}` for formatting", path.display()))?;

    let (indent_char, indent_size) = xml_format_options();

    let mut reader = quick_xml::Reader::from_str(&content);
    reader.config_mut().check_comments = true;
    reader.config_mut().enable_all_checks(true);

    let mut writer = quick_xml::Writer::new_with_indent(Vec::<u8>::new(), indent_char, indent_size);

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
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
