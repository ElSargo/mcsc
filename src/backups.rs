use color_eyre::Result;
use std::{path::PathBuf, time::SystemTime};
use tokio::fs;
use tokio_stream::{wrappers::ReadDirStream, Stream, StreamExt};

pub async fn latest_file(dir: &str) -> Option<PathBuf> {
    let files = fs::read_dir(dir).await.ok()?;
    Some(
        stream_keep_by_key(iter_paths_with_sys_time(files).await, |x, y| x.1 > y.1)
            .await?
            .0,
    )
}

pub async fn stream_keep_by_key<O, T: Stream<Item = O>, F: Fn(&O, &O) -> bool>(
    stream: T,
    f: F,
) -> Option<O> {
    stream
        .fold(None, |acc, x| {
            if let Some(prev) = acc {
                Some(if f(&prev, &x) { prev } else { x })
            } else {
                Some(x)
            }
        })
        .await
}

pub async fn iter_paths_with_sys_time(
    files: fs::ReadDir,
) -> impl Stream<Item = (PathBuf, SystemTime)> {
    let dir_stream = ReadDirStream::new(files);

    dir_stream.filter_map(|r| r.ok()).filter_map(|pb| {
        let p = pb.path();
        let time = match std::fs::metadata(&p) {
            Ok(metadata) => match metadata.modified() {
                Ok(time) => time,
                Err(_) => return None,
            },
            Err(_) => return None,
        };
        Some((p, time))
    })
}

pub async fn remove_oldest_backup(dir: &str) -> Result<()> {
    let files = fs::read_dir(dir).await?;

    if let Some(tuple) =
        stream_keep_by_key(iter_paths_with_sys_time(files).await, |a, b| a.1 < b.1).await
    {
        std::fs::remove_file(tuple.0)?;
    }
    Ok(())
}
