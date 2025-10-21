use std::collections::HashSet;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;
use std::{fs, io};

use indexmap::IndexMap;
use log::warn;
use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};

use crate::Scheme;
use crate::output::FileStatus;

pub(crate) const MANIFEST_PATH: &str = ".basel/manifest.json";
pub(crate) const MANIFEST_VERSION: u8 = 0;

fn manifest_filename() -> &'static str {
    extract_filename_from(MANIFEST_PATH)
}

fn manifest_parents() -> &'static str {
    extract_parents_from(MANIFEST_PATH).map_or(".", |ancestors| ancestors)
}

fn extract_filename_from(path: &str) -> &str {
    if let Some((_, file)) = path.rsplit_once('/') {
        return file;
    }

    path
}

fn extract_parents_from(path: &str) -> Option<&str> {
    if let Some((ancestors, _)) = path.rsplit_once('/') {
        return Some(ancestors);
    }

    None
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("failed to read `{}`: {src}", manifest_filename())]
    Reading { src: io::Error },
    #[error("failed to parse `{}`: {src}", manifest_filename())]
    Parsing { src: Box<serde_json::Error> },
    #[error("failed to create `{}` dir: {src}", manifest_parents())]
    CreatingDir { src: io::Error },
    #[error("failed to write `{MANIFEST_PATH}`: {src}")]
    Writing { src: io::Error },
}

type Result<T> = StdResult<T, Error>;

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ManagedFile {
    pub path: PathBuf,
    pub template: String,
    pub scheme: String,
    pub hash: String,
    pub template_hash: String,
    pub scheme_hash: String,
}

impl ManagedFile {
    const fn new(
        path: PathBuf,
        template: String,
        scheme: String,
        hash: String,
        template_hash: String,
        scheme_hash: String,
    ) -> Self {
        Self {
            path,
            template,
            scheme,
            hash,
            template_hash,
            scheme_hash,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub version: u8,
    #[serde(
        serialize_with = "serialize_files",
        deserialize_with = "deserialize_files"
    )]
    pub files: IndexMap<PathBuf, ManagedFile>,
}

impl Manifest {
    fn new() -> Self {
        Self {
            version: MANIFEST_VERSION,
            files: IndexMap::new(),
        }
    }

    pub(crate) fn load_or_create() -> Result<Self> {
        match fs::read_to_string(MANIFEST_PATH) {
            Ok(content) => Ok(serde_json::from_str(&content)
                .map_err(|src| Error::Parsing { src: Box::new(src) })?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // TODO: only warn if the output directory isn't empty
                warn!(
                    "`{MANIFEST_PATH}` not found, generating new one (all files untracked! all \
                     files in the output directory will be OVERWRITTEN by newly rendered \
                     templates by default!)"
                );

                Ok(Self::new())
            }
            Err(e) => Err(Error::Reading { src: e }),
        }
    }

    pub(crate) fn save(&self) -> Result<()> {
        fs::create_dir_all(manifest_parents()).map_err(|src| Error::CreatingDir { src })?;

        let content = serde_json::to_string_pretty(self)
            .map_err(|src| Error::Parsing { src: Box::new(src) })?;

        fs::write(MANIFEST_PATH, content).map_err(|src| Error::Writing { src })?;

        Ok(())
    }

    #[must_use]
    pub(crate) fn get(&self, path: &Path) -> Option<&ManagedFile> {
        self.files.get(path)
    }

    pub(crate) fn insert(&mut self, file: ManagedFile) -> bool {
        self.files.insert(file.path.clone(), file).is_some()
    }

    pub(crate) fn remove(&mut self, path: &Path) -> bool {
        self.files.swap_remove(path).is_some()
    }

    #[must_use]
    pub(crate) fn find_orphans(&self, rendered_paths: &HashSet<PathBuf>) -> Vec<PathBuf> {
        self.files
            .keys()
            .filter(|path| !rendered_paths.contains(*path))
            .cloned()
            .collect()
    }

    pub(crate) fn check_file(
        &self,
        path: &Path,
        scheme: &Scheme,
        template: &minijinja::Template<'_, '_>,
    ) -> Result<FileStatus> {
        let Some(entry) = self.get(path) else {
            return Ok(FileStatus::NotTracked);
        };

        let file_exists = path.exists();
        let current_hash = if file_exists { hash_file(path) } else { None };

        let user_modified = current_hash.is_some() && current_hash.as_ref() != Some(&entry.hash);
        let template_changed = hash_template(template) != entry.template_hash;
        let scheme_changed = hash_scheme(scheme)? != entry.scheme_hash;

        Ok(FileStatus::Tracked {
            file_exists,
            user_modified,
            template_changed,
            scheme_changed,
        })
    }

    pub(crate) fn make_entry(
        path: PathBuf,
        template: &minijinja::Template<'_, '_>,
        scheme: &Scheme,
        content: &str,
    ) -> Result<ManagedFile> {
        Ok(ManagedFile::new(
            path,
            template.name().to_owned(),
            scheme.name.as_str().to_owned(),
            hash(content),
            hash_template(template),
            hash_scheme(scheme)?,
        ))
    }
}

fn serialize_files<S>(
    files: &IndexMap<PathBuf, ManagedFile>,
    serializer: S,
) -> StdResult<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(files.len()))?;

    for file in files.values() {
        seq.serialize_element(file)?;
    }

    seq.end()
}

fn deserialize_files<'de, D>(deserializer: D) -> StdResult<IndexMap<PathBuf, ManagedFile>, D::Error>
where
    D: Deserializer<'de>,
{
    struct FilesVisitor;

    impl<'de> Visitor<'de> for FilesVisitor {
        type Value = IndexMap<PathBuf, ManagedFile>;

        fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a sequence of managed files")
        }

        fn visit_seq<A>(self, mut seq: A) -> StdResult<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut map = IndexMap::new();

            while let Some(file) = seq.next_element::<ManagedFile>()? {
                map.insert(file.path.clone(), file);
            }

            Ok(map)
        }
    }

    deserializer.deserialize_seq(FilesVisitor)
}

fn hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());

    format!("sha256:{:x}", hasher.finalize())
}

fn hash_file(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|c| hash(&c))
}

fn hash_template(template: &minijinja::Template<'_, '_>) -> String {
    hash(template.source())
}

fn hash_scheme(scheme: &Scheme) -> Result<String> {
    let json = serde_json::to_string_pretty(scheme)
        .map_err(|src| Error::Parsing { src: Box::new(src) })?;

    Ok(hash(&json))
}
