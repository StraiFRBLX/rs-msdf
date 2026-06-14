use thiserror::Error as ThisError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, ThisError)]
pub enum Error {
    #[error("invalid options: {0}")]
    InvalidOptions(String),

    #[error("failed to parse SVG: {0}")]
    SvgParse(#[from] usvg::Error),

    #[error("failed to parse SVG XML: {0}")]
    XmlParse(#[from] roxmltree::Error),

    #[error("unsupported SVG feature: {0}")]
    UnsupportedSvg(String),

    #[error("SVG contains no visible supported geometry")]
    EmptyGeometry,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("PNG encoding error: {0}")]
    PngEncoding(#[from] png::EncodingError),

    #[error("JSON encoding error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("compression error: {0}")]
    Compression(String),

    #[error("glob pattern error: {0}")]
    GlobPattern(#[from] glob::PatternError),

    #[error("glob expansion error: {0}")]
    Glob(#[from] glob::GlobError),

    #[error("thread pool error: {0}")]
    ThreadPool(#[from] rayon::ThreadPoolBuildError),
}
