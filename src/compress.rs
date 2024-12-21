use rkyv::Archive;
use std::io::{Read, Write};

use crate::Error;

const ZSTD_COMPRESSION_LEVEL: i32 = 21;

/// Compression algorithms available for patch data.
///
/// Determines how patch data is compressed before storage or transmission.
/// Different algorithms offer tradeoffs between compression ratio and speed.
///
/// # Example
/// ```rust
/// use darkwing_diff::{diff, DiffAlgorithm, CompressAlgorithm};
///
/// let before = b"original content";
/// let after = b"modified content";
///
/// // Use no compression for debugging or when speed is critical
/// let uncompressed = diff(
///     before,
///     after,
///     DiffAlgorithm::Rsync020,
///     CompressAlgorithm::None
/// )?;
///
/// // Use Zstd for maximum compression
/// let compressed = diff(
///     before,
///     after,
///     DiffAlgorithm::Rsync020,
///     CompressAlgorithm::Zstd
/// )?;
/// # Ok::<(), darkwing_diff::Error>(())
/// ```
#[derive(
  Archive,
  rkyv::Deserialize,
  rkyv::Serialize,
  Debug,
  PartialEq,
  Copy,
  Clone,
  Eq,
  Hash,
)]
#[rkyv(derive(Debug, PartialEq, Copy, Clone))]
pub enum CompressAlgorithm {
  /// No compression. Patch data is stored as-is.
  /// Use this when:
  /// - Debugging patches
  /// - Working with already compressed data
  /// - Speed is more important than size
  None,

  /// Zstandard compression with level 21 (maximum compression).
  /// Use this when:
  /// - Minimizing patch size is critical
  /// - Network bandwidth or storage is limited
  /// - Compression time is not a concern
  Zstd,
}

impl std::fmt::Display for CompressAlgorithm {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:?}", self)
  }
}

impl CompressAlgorithm {
  /// Compresses the input data using the selected algorithm.
  pub fn compress(self, input: &[u8]) -> Result<Vec<u8>, Error> {
    match self {
      Self::None => Ok(input.to_vec()),
      Self::Zstd => {
        let mut encoder =
          zstd::Encoder::new(Vec::new(), ZSTD_COMPRESSION_LEVEL).map_err(
            |e| {
              Error::ZipError(format!("failed to create zstd encoder: {}", e))
            },
          )?;
        encoder
          .write_all(input)
          .map_err(|e| Error::ZipError(format!("failed to write: {}", e)))?;
        Ok(
          encoder
            .finish()
            .map_err(|e| Error::ZipError(format!("failed to finish: {}", e)))?,
        )
      }
    }
  }

  /// Decompresses the input data using the selected algorithm.
  pub(crate) fn decompress(self, input: &[u8]) -> Result<Vec<u8>, Error> {
    match self {
      Self::None => Ok(input.to_vec()),
      Self::Zstd => {
        let mut output = Vec::new();
        let mut decoder = zstd::Decoder::new(input).map_err(|e| {
          Error::ZipError(format!("failed to create zstd decoder: {}", e))
        })?;
        decoder
          .read_to_end(&mut output)
          .map_err(|e| Error::ZipError(format!("failed to read: {}", e)))?;
        Ok(output)
      }
    }
  }
}
