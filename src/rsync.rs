use super::*;

use fast_rsync::Signature;

pub(super) struct RsyncDiffMachine;

const RSYNC_SIGNATURE_OPTIONS: fast_rsync::SignatureOptions =
  fast_rsync::SignatureOptions {
    block_size: 1024,
    crypto_hash_size: 16,
  };

impl DiffMachine for RsyncDiffMachine {
  fn diff(
    before: &[u8],
    after: &[u8],
    compress_algorithm: CompressAlgorithm,
  ) -> Result<Patch, Error> {
    let signature = Signature::calculate(before, RSYNC_SIGNATURE_OPTIONS);
    let signature = signature.index();

    let mut result = Vec::new();
    fast_rsync::diff(&signature, after, &mut result)
      .map_err(Error::RsyncDiffError)?;

    let compressed_patch = compress_algorithm.compress(&result)?;

    let result = Patch {
      diff_algorithm: DiffAlgorithm::Rsync020,
      compress_algorithm,
      before_hash: hash(before),
      after_hash: hash(after),
      patch: compressed_patch,
    };

    Ok(result)
  }

  fn apply(base: &[u8], delta: &Patch) -> Result<Vec<u8>, Error> {
    assert!(delta.diff_algorithm == DiffAlgorithm::Rsync020);

    let base_hash = hash(base);

    if base_hash != delta.before_hash {
      return Err(Error::BeforeHashMismatch);
    }

    let decompressed_patch =
      delta.compress_algorithm.decompress(&delta.patch)?;

    let mut out = Vec::new();

    fast_rsync::apply(base, &decompressed_patch, &mut out)
      .map_err(Error::RsyncApplyError)?;

    let after_hash = hash(&out);
    if after_hash != delta.after_hash {
      return Err(Error::AfterHashMismatch);
    }

    Ok(out)
  }
}
