use darkwing_diff::{
  CompressAlgorithm, DiffAlgorithm, Patch, PatchSet, apply, apply_zip, diff,
  diff_zip,
};
use std::fs;
use std::path::Path;
use std::time::Instant;
use tabled::{Table, Tabled};

#[derive(Debug, Tabled, Clone)]
struct DiffMetrics {
  zip_name: String,
  as_what: AsPatchOrPatchSet,
  diff_algo: DiffAlgorithm,
  compress_algo: CompressAlgorithm,
  diff_time_ms: u128,
  apply_time_ms: u128,
  patch_size_bytes: usize,
  total_second_size: usize,
}

type Variant = (CompressAlgorithm, DiffAlgorithm, AsPatchOrPatchSet);

fn get_combinations() -> Vec<Variant> {
  // let compress_algorithms =
  //   vec![CompressAlgorithm::None, CompressAlgorithm::Zstd];

  // let diff_algorithms = vec![DiffAlgorithm::Rsync020,
  // DiffAlgorithm::Bidiff1];

  // let as_patch_or_patch_set =
  //   vec![AsPatchOrPatchSet::AsPatch, AsPatchOrPatchSet::AsPatchSet];

  // let mut combinations = Vec::with_capacity(
  //   compress_algorithms.len()
  //     * diff_algorithms.len()
  //     * as_patch_or_patch_set.len(),
  // );

  // for compress_algo in compress_algorithms {
  //   for diff_algo in &diff_algorithms {
  //     for as_patch_or_patch_set in &as_patch_or_patch_set {
  //       combinations.push((
  //         compress_algo,
  //         diff_algo.clone(),
  //         as_patch_or_patch_set.clone(),
  //       ));
  //     }
  //   }
  // }

  // lets hand-pick our combinations

  use AsPatchOrPatchSet::*;
  use CompressAlgorithm::*;
  use DiffAlgorithm::*;

  let combinations: Vec<(CompressAlgorithm, DiffAlgorithm, AsPatchOrPatchSet)> = vec![
    (None, Bidiff1, AsPatch), // this combination is VERY low-performant
    // (takes tens of seconds on big files (40MB+))
    (Zstd, Bidiff1, AsPatch), // this combination is VERY low-performant
    // (takes tens of seconds on big files (40MB+))
    (None, Rsync020, AsPatch), // this combination is not so low-performant
    // (its kinda fast), but it generates big patches (10 times bigger than the
    // AsPatchSet variant)
    (Zstd, Rsync020, AsPatch), // this combination is not so low-performant
    // (its kinda fast), but it generates big patches (10 times bigger than the
    // AsPatchSet variant)
    (None, Rsync020, AsPatchSet),
    (None, Bidiff1, AsPatchSet),
    (Zstd, Rsync020, AsPatchSet),
    (Zstd, Bidiff1, AsPatchSet),
  ];

  combinations
}

fn find_most_avg_performant_combination(
  metrics: Vec<DiffMetrics>,
) -> (Variant, Variant) {
  // Group metrics by variant
  let mut variant_metrics: std::collections::HashMap<
    Variant,
    Vec<&DiffMetrics>,
  > = std::collections::HashMap::new();

  for metric in metrics.iter() {
    let variant = (metric.compress_algo, metric.diff_algo, metric.as_what);
    variant_metrics.entry(variant).or_default().push(metric);
  }

  // Calculate average time for each variant
  let mut variant_avg_times: Vec<(Variant, f64)> = variant_metrics
    .iter()
    .map(|(variant, metrics)| {
      let avg_time = metrics
        .iter()
        .map(|m| (m.diff_time_ms + m.apply_time_ms) as f64)
        .sum::<f64>()
        / metrics.len() as f64;
      (*variant, avg_time)
    })
    .collect();

  // Sort by average time and get the fastest variant
  variant_avg_times
    .sort_by(|a, b| a.1.partial_cmp(&b.1).expect("No NaN values expected"));

  // Calculate average size reduction for each variant
  let mut variant_avg_size_reduction: Vec<(Variant, f64)> = variant_metrics
    .iter()
    .map(|(variant, metrics)| {
      let avg_size_reduction = metrics
        .iter()
        .map(|m| m.patch_size_bytes as f64 / m.total_second_size as f64)
        .sum::<f64>()
        / metrics.len() as f64;
      (*variant, avg_size_reduction)
    })
    .collect();

  // Sort by average time and get the fastest variant
  variant_avg_size_reduction
    .sort_by(|a, b| a.1.partial_cmp(&b.1).expect("No NaN values expected"));

  (
    variant_avg_times.first().unwrap().0,
    variant_avg_size_reduction.first().unwrap().0,
  )
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
enum AsPatchOrPatchSet {
  AsPatch,
  AsPatchSet,
}

impl std::fmt::Display for AsPatchOrPatchSet {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      AsPatchOrPatchSet::AsPatch => write!(f, "as one file"),
      AsPatchOrPatchSet::AsPatchSet => write!(f, "per-file"),
    }
  }
}

enum PatchOrPatchSet {
  Patch(Patch),
  PatchSet(PatchSet),
}

impl PatchOrPatchSet {
  fn get_size(&self) -> usize {
    match self {
      PatchOrPatchSet::Patch(patch) => patch.get_size(),
      PatchOrPatchSet::PatchSet(patch_set) => patch_set.get_size(),
    }
  }
}

impl From<Patch> for PatchOrPatchSet {
  fn from(patch: Patch) -> Self {
    PatchOrPatchSet::Patch(patch)
  }
}

impl From<PatchSet> for PatchOrPatchSet {
  fn from(patch_set: PatchSet) -> Self {
    PatchOrPatchSet::PatchSet(patch_set)
  }
}

fn measure_diff_roundtrip(
  compress_algo: CompressAlgorithm,
  diff_algo: DiffAlgorithm,
  as_patch_or_patch_set: AsPatchOrPatchSet,
  original_path: &str,
  modified_path: &str,
  applied_path: &str,
) -> Result<DiffMetrics, Box<dyn std::error::Error>> {
  // Measure diff time
  let diff_start = Instant::now();
  let patch: PatchOrPatchSet = match as_patch_or_patch_set {
    AsPatchOrPatchSet::AsPatch => {
      // Read files
      let original = fs::read(original_path)?;
      let modified = fs::read(modified_path)?;

      diff(&original, &modified, diff_algo, compress_algo)
        .unwrap()
        .into()
    }
    AsPatchOrPatchSet::AsPatchSet => diff_zip(
      original_path.into(),
      modified_path.into(),
      diff_algo,
      compress_algo,
    )
    .unwrap()
    .into(),
  };
  let diff_time = diff_start.elapsed();

  // Get patch size
  let patch_size = patch.get_size();

  // Measure apply time
  let apply_start = Instant::now();
  match patch {
    PatchOrPatchSet::Patch(patch) => {
      let original = fs::read(original_path)?;
      apply(&original, &patch).unwrap()
    }
    PatchOrPatchSet::PatchSet(patch_set) => {
      apply_zip(original_path, patch_set, applied_path.into()).unwrap();
      fs::read(applied_path)?
    }
  };
  let apply_time = apply_start.elapsed();

  let zip_name = Path::new(original_path)
    .file_name()
    .unwrap()
    .to_str()
    .unwrap()
    .to_string()
    + "_"
    + Path::new(modified_path)
      .file_name()
      .unwrap()
      .to_str()
      .unwrap();

  let total_second_size = fs::metadata(modified_path)?.len();

  Ok(DiffMetrics {
    zip_name,
    diff_time_ms: diff_time.as_millis(),
    apply_time_ms: apply_time.as_millis(),
    patch_size_bytes: patch_size,
    total_second_size: total_second_size as usize,
    compress_algo,
    diff_algo,
    as_what: as_patch_or_patch_set,
  })
}

pub fn main() {
  if cfg!(debug_assertions) {
    println!(
      "Debug mode should not be used as this program is used for benchmarking and it can cause wrong results. 
      Please run with --release flag. Exiting now."
    );
    std::process::exit(0);
  }

  let variants = get_combinations();
  let big_sized_datadirs = [
    (
      "tests/data/lot-of-datadirs/278706406.datadir.zip.before",
      "tests/data/lot-of-datadirs/278706406.datadir.zip.after",
    ),
    (
      "tests/data/lot-of-datadirs/94725013.datadir.zip.before",
      "tests/data/lot-of-datadirs/94725013.datadir.zip.after",
    ),
    (
      "tests/data/lot-of-datadirs/107671255.datadir.zip.before",
      "tests/data/lot-of-datadirs/107671255.datadir.zip.after",
    ),
    (
      "tests/data/lot-of-datadirs/151597589.datadir.zip.before",
      "tests/data/lot-of-datadirs/151597589.datadir.zip.after",
    ),
    (
      "tests/data/lot-of-datadirs/455887522.datadir.zip.before",
      "tests/data/lot-of-datadirs/455887522.datadir.zip.after",
    ),
    (
      "tests/data/lot-of-datadirs/497238892.datadir.zip.before",
      "tests/data/lot-of-datadirs/497238892.datadir.zip.after",
    ),
    (
      "tests/data/lot-of-datadirs/497238892.datadir.zip.after",
      "tests/data/lot-of-datadirs/497238892.datadir.zip.after2",
    ),
  ];
  let mut metrics: Vec<DiffMetrics> = Vec::new();

  for variant in variants.clone() {
    for (before_file, after_file) in big_sized_datadirs {
      let res = measure_diff_roundtrip(
        variant.0,
        variant.1,
        variant.2,
        before_file,
        after_file,
        &format!("{}.COPY_FOR_TESTS", after_file),
      )
      .unwrap();

      metrics.push(res);
    }
  }

  let table = Table::new(metrics.clone());
  println!("{}", table);

  println!("\n");

  for variant in variants {
    print!("variant {:?}, ", variant);
    println!(
      "avg size change: {:+.4}%, avg speed: {:+.4} ms",
      find_avg_size_reduction(metrics.clone(), variant),
      find_avg_speed(metrics.clone(), variant)
    );
  }

  println!("\n");

  let best = find_most_avg_performant_combination(metrics.clone());
  println!(
    "Best for speed in big-sized-datadirs: {:?}. Avg size change: {:+.4}%. Avg speed: {:+.4} ms",
    best.0,
    find_avg_size_reduction(metrics.clone(), best.0),
    find_avg_speed(metrics.clone(), best.0)
  );
  println!(
    "Best for size in big-sized-datadirs: {:?}. Avg size change: {:+.4}%. Avg speed: {:+.4} ms",
    best.1,
    find_avg_size_reduction(metrics.clone(), best.1),
    find_avg_speed(metrics.clone(), best.1)
  );

  let small_sized_datadirs = [
    (
      "tests/data/datadir-v19-pixelscan.zip",
      "tests/data/datadir-v23-random.zip",
    ),
    (
      "tests/data/lot-of-datadirs/497239205.datadir.zip.before",
      "tests/data/lot-of-datadirs/497239205.datadir.zip.after",
    ),
    (
      "tests/data/lot-of-datadirs/497238907.datadir.zip.before",
      "tests/data/lot-of-datadirs/497238907.datadir.zip.after",
    ),
  ];
  let mut metrics: Vec<DiffMetrics> = Vec::new();
  let variants = get_combinations();

  for variant in variants.clone() {
    for (before_file, after_file) in small_sized_datadirs {
      let res = measure_diff_roundtrip(
        variant.0,
        variant.1,
        variant.2,
        before_file,
        after_file,
        &format!("{}.COPY_FOR_TESTS", after_file),
      )
      .unwrap();

      metrics.push(res);
    }
  }

  let table = Table::new(metrics.clone());
  println!("{}", table);

  println!("\n");

  for variant in variants {
    print!("variant {:?}, ", variant);
    println!(
      "avg size change: {:+.4}%, avg speed: {:+.4} ms",
      find_avg_size_reduction(metrics.clone(), variant),
      find_avg_speed(metrics.clone(), variant)
    );
  }

  println!("\n");

  let best = find_most_avg_performant_combination(metrics.clone());
  println!(
    "Best for speed in small-sized-datadirs: {:?}. Avg size change: {:+.4}%. Avg speed: {:+.4} ms",
    best.0,
    find_avg_size_reduction(metrics.clone(), best.0),
    find_avg_speed(metrics.clone(), best.0)
  );
  println!(
    "Best for size in small-sized-datadirs: {:?}. Avg size change: {:+.4}%. Avg speed: {:+.4} ms",
    best.1,
    find_avg_size_reduction(metrics.clone(), best.1),
    find_avg_speed(metrics.clone(), best.1)
  );
}

fn find_avg_size_reduction(metrics: Vec<DiffMetrics>, variant: Variant) -> f64 {
  let filtered_metrics: Vec<_> = metrics
    .iter()
    .filter(|m| {
      m.compress_algo == variant.0
        && m.diff_algo == variant.1
        && m.as_what == variant.2
    })
    .collect();

  if filtered_metrics.is_empty() {
    return 0.0;
  }

  (filtered_metrics
    .iter()
    .map(|m| (m.patch_size_bytes as f64 / m.total_second_size as f64) - 1.0)
    .sum::<f64>()
    / filtered_metrics.len() as f64)
    * 100.0
}

fn find_avg_speed(metrics: Vec<DiffMetrics>, variant: Variant) -> f64 {
  let filtered_metrics: Vec<_> = metrics
    .iter()
    .filter(|m| {
      m.compress_algo == variant.0
        && m.diff_algo == variant.1
        && m.as_what == variant.2
    })
    .collect();

  if filtered_metrics.is_empty() {
    return 0.0;
  }

  filtered_metrics
    .iter()
    .map(|m| (m.diff_time_ms + m.apply_time_ms) as f64)
    .sum::<f64>()
    / filtered_metrics.len() as f64
}
