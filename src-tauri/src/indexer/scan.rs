use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
        }
    }
}

const IMAGE_EXT: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif",
];

/// Camera RAW formats. Most cannot be decoded for thumbnails, which is handled
/// gracefully (the asset is indexed and shown with a placeholder).
pub const RAW_EXT: &[&str] = &[
    "raw", "dng", "cr2", "cr3", "nef", "nrw", "arw", "srf", "sr2", "orf", "raf", "rw2", "pef",
    "srw", "x3f", "3fr", "erf", "mrw", "dcr", "kdc",
];

const VIDEO_EXT: &[&str] = &["mp4", "mov", "m4v", "avi", "mkv", "webm"];

pub fn media_type_for_path(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if IMAGE_EXT.contains(&ext.as_str()) || RAW_EXT.contains(&ext.as_str()) {
        Some(MediaKind::Image)
    } else if VIDEO_EXT.contains(&ext.as_str()) {
        Some(MediaKind::Video)
    } else {
        None
    }
}

pub fn is_supported_media(path: &Path) -> bool {
    media_type_for_path(path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_image_and_video() {
        assert_eq!(
            media_type_for_path(&PathBuf::from("a.JPG")),
            Some(MediaKind::Image)
        );
        assert_eq!(
            media_type_for_path(&PathBuf::from("clip.mp4")),
            Some(MediaKind::Video)
        );
        assert_eq!(media_type_for_path(&PathBuf::from("notes.txt")), None);
    }
}
