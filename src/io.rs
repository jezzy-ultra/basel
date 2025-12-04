use std::path::Path;

use anyhow::Context as _;

pub(crate) async fn read(path: impl AsRef<Path>) -> anyhow::Result<String> {
    tokio::fs::read_to_string(path.as_ref())
        .await
        .with_context(|| format!("reading `{}`", path.as_ref().display()))
}

pub(crate) async fn try_read(path: impl AsRef<Path>) -> anyhow::Result<Option<String>> {
    match tokio::fs::read_to_string(path.as_ref()).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::from(e))
            .with_context(|| format!("reading `{}`", path.as_ref().display())),
    }
}

pub(crate) async fn write(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> anyhow::Result<()> {
    tokio::fs::write(path.as_ref(), contents)
        .await
        .with_context(|| format!("writing `{}`", path.as_ref().display()))
}

pub(crate) async fn create_parent_all(path: impl AsRef<Path>) -> anyhow::Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        tokio::fs::create_dir_all(parent)
            .await
            // TODO: handle pluralization more gracefully
            .with_context(|| format!("creating parent(s) for `{}`", path.as_ref().display()))?;
    }

    Ok(())
}

pub(crate) async fn remove_dir_all(path: impl AsRef<Path>) -> anyhow::Result<()> {
    tokio::fs::remove_dir_all(path.as_ref())
        .await
        // TODO: handle pluralization more gracefully
        .with_context(|| format!("removing directory(s) `{}`", path.as_ref().display()))
}
