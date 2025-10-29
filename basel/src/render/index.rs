use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::output::FileStatus;
use crate::{Manifest, ManifestEntry, Scheme, manifest};

pub(super) type Index = Manifest<Entry>;

impl Index {
    pub(crate) fn check(
        &self,
        path: &Path,
        scheme: &Scheme,
        template: &minijinja::Template<'_, '_>,
    ) -> anyhow::Result<FileStatus> {
        let Some(entry) = self.get(path) else {
            return Ok(FileStatus::NotTracked);
        };

        manifest::check_status(path, &entry.hash, || {
            Ok(hash_template(template) != entry.template_hash
                || hash_scheme(scheme)? != entry.scheme_hash)
        })
    }

    pub(crate) fn create_entry(
        path: &Path,
        template: &minijinja::Template<'_, '_>,
        scheme: &Scheme,
        content: &str,
    ) -> anyhow::Result<Entry> {
        Ok(Entry {
            path: path.to_path_buf(),
            template: template.name().to_owned(),
            scheme: scheme.name.as_str().to_owned(),
            hash: manifest::hash(content),
            template_hash: hash_template(template),
            scheme_hash: hash_scheme(scheme)?,
        })
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub path: PathBuf,
    pub template: String,
    pub scheme: String,
    pub hash: String,
    pub template_hash: String,
    pub scheme_hash: String,
}

impl ManifestEntry for Entry {
    const FILENAME: &'static str = "index.json";
    const VERSION: u8 = 0;

    fn path(&self) -> &Path {
        &self.path
    }

    fn hash(&self) -> &str {
        &self.hash
    }
}

fn hash_template(template: &minijinja::Template<'_, '_>) -> String {
    manifest::hash(template.source())
}

fn hash_scheme(scheme: &Scheme) -> anyhow::Result<String> {
    let json = serde_json::to_string_pretty(scheme)?;

    Ok(manifest::hash(&json))
}
