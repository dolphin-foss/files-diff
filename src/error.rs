/// Errors that can occur during diffing and patching operations.
///
/// This enum represents all possible errors that can occur when generating or
/// applying patches, including algorithm-specific errors, hash validation
/// failures, and I/O operations.
///
/// # Example
/// ```rust
/// use files_diff::{diff, DiffAlgorithm, CompressAlgorithm};
///
/// let before = b"original";
/// let after = b"modified";
///
/// match diff(before, after, DiffAlgorithm::Rsync020, CompressAlgorithm::Zstd) {
///     Ok(patch) => println!("Patch generated successfully"),
///     Err(e) => match e {
///         files_diff::Error::IoError(msg) => eprintln!("IO error: {}", msg),
///         files_diff::Error::BeforeHashMismatch => eprintln!("Source file corrupted"),
///         _ => eprintln!("Other error: {:?}", e),
///     }
/// }
/// ```
#[derive(Debug)]
pub enum Error {
  /// An error occurred in the rsync diff algorithm
  RsyncDiffError(fast_rsync::DiffError),

  /// An error occurred while applying an rsync patch
  RsyncApplyError(fast_rsync::ApplyError),

  /// An error occurred in the bidiff algorithm
  BidiffError(String),

  /// The hash of the source file doesn't match the expected hash
  BeforeHashMismatch,

  /// The hash of the generated file doesn't match the expected hash
  AfterHashMismatch,

  /// The hash of the operations doesn't match the expected hash
  OperationsHashMismatch,

  /// An I/O error occurred during file operations
  IoError(String),

  /// An error occurred while processing a zip archive
  ZipError(String),

  /// An error occurred while serializing a patch or patch set
  SerializeError(rkyv::rancor::Error),

  /// An error occurred while deserializing a patch or patch set
  DeserializeError(rkyv::rancor::Error),
}
