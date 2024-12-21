use rkyv::Archive;

use crate::{Error, compress::CompressAlgorithm, hash};

/// Algorithms available for generating binary diffs.
///
/// Each algorithm offers different tradeoffs between patch size, generation
/// speed, and application speed.
///
/// # Example
/// ```rust
/// use files_diff::{diff, DiffAlgorithm, CompressAlgorithm};
///
/// // Use rsync for fast diffing of similar files
/// let rsync_patch = diff(
///     b"original",
///     b"modified",
///     DiffAlgorithm::Rsync020,
///     CompressAlgorithm::None
/// )?;
///
/// // Use bidiff for potentially smaller patches
/// let bidiff_patch = diff(
///     b"original",
///     b"modified",
///     DiffAlgorithm::Bidiff1,
///     CompressAlgorithm::Zstd
/// )?;
/// # Ok::<(), files_diff::Error>(())
/// ```
#[derive(
  Archive,
  rkyv::Deserialize,
  rkyv::Serialize,
  Debug,
  PartialEq,
  Clone,
  Copy,
  Eq,
  Hash,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum DiffAlgorithm {
  /// Fast-rsync algorithm version 0.2.0.
  /// Optimized for files that are mostly similar.
  Rsync020,

  /// Bidirectional diff algorithm version 1.
  /// May produce smaller patches for very different files.
  Bidiff1,
}

impl std::fmt::Display for DiffAlgorithm {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:?}", self)
  }
}

/// A patch that can transform one file into another.
///
/// Contains all the information needed to verify and apply a patch,
/// including source and target file hashes for integrity validation.
///
/// # Example
/// ```rust
/// use files_diff::{diff, apply, DiffAlgorithm, CompressAlgorithm};
///
/// let source = b"original content";
/// let target = b"modified content";
///
/// // Generate a patch
/// let patch = diff(
///     source,
///     target,
///     DiffAlgorithm::Rsync020,
///     CompressAlgorithm::Zstd
/// )?;
///
/// // Verify source hash matches
/// assert_eq!(files_diff::hash(source), patch.before_hash);
///
/// // Apply patch and verify result
/// let result = apply(source, &patch)?;
/// assert_eq!(files_diff::hash(&result), patch.after_hash);
/// # Ok::<(), files_diff::Error>(())
/// ```
#[derive(Archive, rkyv::Deserialize, rkyv::Serialize, Debug, PartialEq)]
#[rkyv(derive(Debug))]
pub struct Patch {
  /// Algorithm used to generate this patch
  pub diff_algorithm: DiffAlgorithm,
  /// Compression method used for the patch data
  pub compress_algorithm: CompressAlgorithm,
  /// MD5 hash of the source file
  pub before_hash: String,
  /// MD5 hash of the target file
  pub after_hash: String,
  /// The actual patch data
  pub patch: Vec<u8>,
}

impl Patch {
  /// Returns the total size in bytes of this patch.
  pub fn get_size(&self) -> usize {
    self.patch.len()
      + self.before_hash.len()
      + self.after_hash.len()
      + std::mem::size_of::<CompressAlgorithm>()
      + std::mem::size_of::<DiffAlgorithm>()
  }

  /// Serializes this patch to a byte vector.
  pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
    Ok(
      rkyv::to_bytes::<rkyv::rancor::Error>(self)
        .map_err(Error::SerializeError)?
        .to_vec(),
    )
  }

  /// Deserializes a patch from a byte vector.
  pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
    rkyv::from_bytes::<_, rkyv::rancor::Error>(bytes)
      .map_err(Error::DeserializeError)
  }
}

/// Type alias for filenames in patch sets
pub type Filename = String;

/// Operations that can be performed on a file in a patch set.
///
/// Used primarily for zip archive diffing to track changes to individual files
/// within the archive.
///
/// # Example
/// ```no_run
/// use files_diff::{diff_zip, DiffAlgorithm, CompressAlgorithm};
///
/// let patch_set = diff_zip(
///     "before.zip".to_string(),
///     "after.zip".to_string(),
///     DiffAlgorithm::Rsync020,
///     CompressAlgorithm::Zstd
/// )?;
///
/// # Ok::<(), files_diff::Error>(())
/// ```
#[derive(Archive, rkyv::Deserialize, rkyv::Serialize, Debug, PartialEq)]
#[rkyv(derive(Debug))]
pub enum Operation {
  /// File was modified - contains patch to transform it
  Patch(Patch),
  /// File is new or completely different - contains full file contents
  PutFile(Vec<u8>),
  /// File was removed in the target
  DeleteFile,
  /// File is identical in source and target
  FileStaysSame,
}

impl Operation {
  /// Returns the size in bytes of this operation's data.
  pub fn get_size(&self) -> usize {
    match self {
      Operation::Patch(patch) => patch.get_size(),
      Operation::PutFile(file) => file.len(),
      Operation::DeleteFile => 0,
      Operation::FileStaysSame => 0,
    }
  }
}

#[derive(Archive, rkyv::Deserialize, rkyv::Serialize, Debug, PartialEq)]
#[rkyv(derive(Debug))]
pub struct Operations(pub(crate) Vec<(Filename, Operation)>);

impl Operations {
  pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, Error> {
    Ok(
      rkyv::to_bytes::<rkyv::rancor::Error>(self)
        .map_err(Error::SerializeError)?
        .to_vec(),
    )
  }

  pub(crate) fn hash(&self) -> Result<String, Error> {
    Ok(hash(&self.to_bytes()?))
  }
}

/// A collection of file operations that transform one archive into another.
///
/// Contains all the operations needed to transform a zip archive into a
/// target version, tracking changes to individual files within the archive.
///
/// # Example
/// ```no_run
/// use files_diff::{diff_zip, apply_zip, DiffAlgorithm, CompressAlgorithm};
///
/// // Generate patches for all files in the zip
/// let patch_set = diff_zip(
///     "source.zip".to_string(),
///     "target.zip".to_string(),
///     DiffAlgorithm::Rsync020,
///     CompressAlgorithm::Zstd
/// )?;
///
/// // Apply all patches to transform the zip
/// apply_zip("source.zip", patch_set, "result.zip".to_string())?;
/// # Ok::<(), files_diff::Error>(())
/// ```
#[derive(Archive, rkyv::Deserialize, rkyv::Serialize, Debug, PartialEq)]
#[rkyv(derive(Debug))]
pub struct PatchSet {
  /// The operations that transform the source zip into the target zip
  pub operations: Operations,
  /// The hash of the source zip
  pub hash_before: String,
  /// The hash of the operations
  pub operations_hash: String,
}

impl PatchSet {
  /// Returns the total size in bytes of all operations in this patch set.
  pub fn get_size(&self) -> usize {
    self
      .operations
      .0
      .iter()
      .map(|(filename, op)| filename.len() + op.get_size())
      .sum::<usize>()
      + self.hash_before.len()
      + self.operations_hash.len()
  }

  /// Serializes this patch set to a byte vector.
  pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
    Ok(
      rkyv::to_bytes::<rkyv::rancor::Error>(self)
        .map_err(Error::SerializeError)?
        .to_vec(),
    )
  }

  /// Deserializes a patch set from a byte vector.
  pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
    rkyv::from_bytes::<_, rkyv::rancor::Error>(bytes)
      .map_err(Error::DeserializeError)
  }
}
