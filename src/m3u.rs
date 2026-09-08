use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub title: Option<String>,
    pub tracks: Vec<PlaylistTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistTrack {
    pub path: PathBuf,
    pub title: Option<String>,
}

pub fn read_playlist(path: &Path) -> Result<Playlist, M3uError> {
    let contents = fs::read_to_string(path).map_err(|source| M3uError::Read {
        path: path.display().to_string(),
        source,
    })?;

    parse_playlist(&contents, path.parent().unwrap_or_else(|| Path::new("")))
}

fn parse_playlist(contents: &str, base_dir: &Path) -> Result<Playlist, M3uError> {
    let mut playlist_title = None;
    let mut playlist_artist = None;
    let mut playlist_album = None;
    let mut pending_track_title = None;
    let mut tracks = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(title) = line.strip_prefix("#PLAYLIST:") {
            playlist_title = non_empty(title);
            continue;
        }

        if let Some(artist) = line.strip_prefix("#EXTART:") {
            playlist_artist = non_empty(artist);
            continue;
        }

        if let Some(album) = line.strip_prefix("#EXTALB:") {
            playlist_album = non_empty(album);
            continue;
        }

        if let Some(extinf) = line.strip_prefix("#EXTINF:") {
            pending_track_title = extinf
                .split_once(',')
                .and_then(|(_duration, title)| non_empty(title));
            continue;
        }

        if line.starts_with('#') {
            continue;
        }

        tracks.push(PlaylistTrack {
            path: resolve_entry_path(base_dir, line),
            title: pending_track_title.take(),
        });
    }

    if tracks.is_empty() {
        return Err(M3uError::NoTracks);
    }

    Ok(Playlist {
        title: playlist_title.or_else(|| artist_album_title(playlist_artist, playlist_album)),
        tracks,
    })
}

fn resolve_entry_path(base_dir: &Path, entry: &str) -> PathBuf {
    let path = PathBuf::from(entry);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn artist_album_title(artist: Option<String>, album: Option<String>) -> Option<String> {
    match (artist, album) {
        (Some(artist), Some(album)) => Some(format!("{artist} - {album}")),
        (Some(artist), None) => Some(artist),
        (None, Some(album)) => Some(album),
        (None, None) => None,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum M3uError {
    #[error("could not read M3U playlist `{path}`: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },

    #[error("M3U playlist does not contain any local track entries")]
    NoTracks,
}

#[cfg(test)]
mod tests {
    use super::{parse_playlist, PlaylistTrack};
    use std::path::Path;

    #[test]
    fn parses_playlist_title_and_track_titles() {
        let playlist = parse_playlist(
            "#EXTM3U\n#PLAYLIST:Artist - Title\n#EXTINF:123,Artist - One\none.flac\n#EXTINF:-1,Two\nmusic/two.mp3\n",
            Path::new("/music"),
        )
        .unwrap();

        assert_eq!(playlist.title.as_deref(), Some("Artist - Title"));
        assert_eq!(
            playlist.tracks,
            vec![
                PlaylistTrack {
                    path: Path::new("/music/one.flac").to_path_buf(),
                    title: Some("Artist - One".to_string()),
                },
                PlaylistTrack {
                    path: Path::new("/music/music/two.mp3").to_path_buf(),
                    title: Some("Two".to_string()),
                },
            ]
        );
    }

    #[test]
    fn derives_playlist_title_from_artist_and_album_tags() {
        let playlist = parse_playlist(
            "#EXTM3U\n#EXTART:Artist\n#EXTALB:Title\ntrack.wav\n",
            Path::new("/music"),
        )
        .unwrap();

        assert_eq!(playlist.title.as_deref(), Some("Artist - Title"));
    }

    #[test]
    fn rejects_playlist_without_tracks() {
        assert!(parse_playlist("#EXTM3U\n#PLAYLIST:Empty\n", Path::new("/music")).is_err());
    }
}
