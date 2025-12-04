use std::collections::HashSet;
use std::fmt::Formatter;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;
use std::{fmt, fs, io};

use anyhow::Context as _;
use indexmap::IndexMap;
use serde::de::{DeserializeOwned, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};

use crate::output::FileStatus;

const DIR: &str = ".theymer";

pub(crate) type Result<T> = StdResult<T, Error>;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("failed to read `{file}`: {src}")]
    Reading { file: String, src: io::Error },

    #[error("failed to parse `{file}`: {src}")]
    Parsing {
        file: String,
        src: Box<serde_json::Error>,
    },

    #[error("failed to create `{path}` dir: {src}")]
    CreatingDir { path: String, src: io::Error },

    #[error("failed to write `{file}`: {src}")]
    Writing { file: String, src: io::Error },
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Manifest<E: Entry> {
    pub version: u8,

    #[serde(
        serialize_with = "serialize_entries",
        deserialize_with = "deserialize_entries"
    )]
    pub entries: IndexMap<PathBuf, E>,
}

pub(crate) trait Entry: Clone + Serialize + DeserializeOwned {
    const FILENAME: &'static str;
    const VERSION: u8;

    fn path(&self) -> &Path;
    fn hash(&self) -> &str;
}

impl<E: Entry> Manifest<E> {
    pub(crate) fn new(version: u8) -> Self {
        Self {
            version,
            entries: IndexMap::new(),
        }
    }

    pub(crate) fn load_or_create() -> Result<Self> {
        match fs::read_to_string(Self::file()) {
            Ok(content) => serde_json::from_str(&content).map_err(|src| Error::Parsing {
                file: Self::file().display().to_string(),
                src: Box::new(src),
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::new(E::VERSION)),
            Err(src) => Err(Error::Reading {
                file: Self::file().display().to_string(),
                src,
            }),
        }
    }

    pub(crate) fn save(&self) -> Result<()> {
        if let Some(parent) = Self::file().parent() {
            fs::create_dir_all(parent).map_err(|src| Error::CreatingDir {
                path: DIR.to_owned(),
                src,
            })?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|src| Error::Parsing {
            file: Self::file().display().to_string(),
            src: Box::new(src),
        })?;

        fs::write(Self::file(), content).map_err(|src| Error::Writing {
            file: Self::file().display().to_string(),
            src,
        })
    }

    pub(crate) fn get(&self, path: &Path) -> Option<&E> {
        self.entries.get(path)
    }

    pub(crate) fn insert(&mut self, entry: E) {
        self.entries.insert(entry.path().to_owned(), entry);
    }

    pub(crate) fn remove(&mut self, path: &Path) -> bool {
        self.entries.swap_remove(path).is_some()
    }

    #[must_use]
    pub(crate) fn find_orphans(&self, paths: &HashSet<PathBuf>) -> Vec<PathBuf> {
        self.entries
            .keys()
            .filter(|p| !paths.contains(*p))
            .cloned()
            .collect()
    }

    fn file() -> PathBuf {
        Path::new(DIR).join(E::FILENAME)
    }
}

pub(crate) fn check_status<F>(
    path: &Path,
    entry_hash: &str,
    dependency_changed: F,
) -> anyhow::Result<FileStatus>
where
    F: FnOnce() -> anyhow::Result<bool>,
{
    let file_exists = path.exists();

    let user_modified = if file_exists {
        let current_hash = hash_file(path)?;

        current_hash != entry_hash
    } else {
        false
    };

    Ok(FileStatus::Tracked {
        file_exists,
        user_modified,
        dependency_changed: dependency_changed()?,
    })
}

pub(crate) fn hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());

    format!("sha256:{:x}", hasher.finalize())
}

pub(crate) fn hash_file(path: &Path) -> anyhow::Result<String> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;

    Ok(hash(&content))
}

fn serialize_entries<E, S>(
    entries: &IndexMap<PathBuf, E>,
    serializer: S,
) -> StdResult<S::Ok, S::Error>
where
    E: Serialize,
    S: Serializer,
{
    use serde::ser::SerializeSeq as _;

    let mut seq = serializer.serialize_seq(Some(entries.len()))?;

    for entry in entries.values() {
        seq.serialize_element(entry)?;
    }

    seq.end()
}

fn deserialize_entries<'de, E, D>(deserializer: D) -> StdResult<IndexMap<PathBuf, E>, D::Error>
where
    E: Entry,
    D: Deserializer<'de>,
{
    struct EntriesVisitor<E>(PhantomData<E>);

    impl<'de, E: Entry> Visitor<'de> for EntriesVisitor<E> {
        type Value = IndexMap<PathBuf, E>;

        fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
            formatter.write_str("a sequence of manifest entries")
        }

        fn visit_seq<A>(self, mut seq: A) -> StdResult<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut map = IndexMap::new();
            while let Some(entry) = seq.next_element::<E>()? {
                map.insert(entry.path().to_owned(), entry);
            }

            Ok(map)
        }
    }

    deserializer.deserialize_seq(EntriesVisitor(PhantomData))
}
