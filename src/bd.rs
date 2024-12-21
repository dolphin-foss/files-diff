use std::io::{BufWriter, Write};

use super::*;

use bidiff::DiffParams;

pub(super) struct BidiffDiffMachine;

// Generally, that will be (num_cpus - 1), leaving one core free for bookkeeping
// and other tasks, but as we do not know the number of cores on specific
// machines, we use 3, assuming everyone in 2024 has at least 4 cores
const SORT_PARTITIONS: usize = 3;
// 512KiB. Choosing a chunk size that's too large will result in suboptimal core
// utilization, whereas choosing a chunk size that's too small will result in
// increased memory usage for diminishing returns
const SCAN_CHUNK_SIZE: usize = 1024 * 512;

impl DiffMachine for BidiffDiffMachine {
  fn diff(
    before: &[u8],
    after: &[u8],
    compress_algorithm: CompressAlgorithm,
  ) -> Result<Patch, Error> {
    let mut patch = std::io::Cursor::new(vec![]);

    let diff_params = DiffParams::new(SORT_PARTITIONS, Some(SCAN_CHUNK_SIZE))
      .map_err(|e| {
      Error::BidiffError(format!("failed to create diff params: {}", e))
    })?;

    bidiff::simple_diff_with_params(before, after, &mut patch, &diff_params)
      .map_err(|e| Error::BidiffError(format!("failed to diff: {}", e)))?;

    patch
      .flush()
      .map_err(|e| Error::IoError(format!("failed to flush: {}", e)))?;

    let compressed_patch = compress_algorithm.compress(patch.get_ref())?;

    Ok(Patch {
      patch: compressed_patch,
      compress_algorithm,
      diff_algorithm: DiffAlgorithm::Bidiff1,
      before_hash: hash(before),
      after_hash: hash(after),
    })
  }

  fn apply(base: &[u8], delta: &Patch) -> Result<Vec<u8>, Error> {
    let hash_before = hash(base);

    if hash_before != delta.before_hash {
      return Err(Error::BeforeHashMismatch);
    }

    let patch = delta
      .compress_algorithm
      .decompress(delta.patch.as_slice())?;

    let patch_reader = std::io::Cursor::new(patch);

    let base_cursor = std::io::Cursor::new(base);

    let mut fresh_r =
      bipatch::Reader::new(patch_reader, base_cursor).map_err(|e| {
        Error::BidiffError(format!("failed to create bidiff reader: {}", e))
      })?;
    let mut output_w = BufWriter::new(vec![]);
    std::io::copy(&mut fresh_r, &mut output_w)
      .map_err(|e| Error::BidiffError(format!("failed to copy: {}", e)))?;
    let after = output_w.into_inner().map_err(|e| {
      Error::BidiffError(format!("failed to get inner of bidiff reader: {}", e))
    })?;

    let hash_after = hash(&after);
    if hash_after != delta.after_hash {
      return Err(Error::AfterHashMismatch);
    }

    Ok(after)
  }
}
