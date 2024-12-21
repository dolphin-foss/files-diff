# darkwing_diff

A high-performance binary diffing library for files and ZIP archives with a focus on stability, extensive testing, and a convenient developer interface.

## Key Features

### ZIP Archive Handling

- Custom PatchSet abstraction for efficient ZIP archive diffing
- Intelligent handling of file additions, deletions, and modifications
- Preserves directory structure while only patching changed files
- Built-in serialization support for patch storage and transmission

### Developer-Friendly Interface

Unlike other low-level diffing libraries, darkwing_diff provides an intuitive API that simplifies common operations while maintaining high performance:

```rust
// Simple one-line diffing
let patch = diff(before, after, DiffAlgorithm::Rsync020, CompressAlgorithm::Zstd)?;

// Straightforward ZIP archive handling
let patch_set = diff_zip("before.zip", "after.zip", diff_algo, compress_algo)?;
```

### Production-Ready Reliability

- Comprehensive test suite including unit, integration, and property-based tests
- Continuous fuzz testing to identify edge cases and potential vulnerabilities
- Built-in integrity validation using MD5 hashes
- Strict versioning and stability guarantees

## Supported Features

- Multiple diffing algorithms

  - Rsync020: Optimized for files that are mostly similar (based on fast-rsync)
  - Bidiff1: Better for files with significant differences (based on bidiff)

- Compression options

  - None: No compression, fastest performance
  - Zstd: High compression ratio using Zstandard level 21

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
darkwing_diff = "0.1.0"
```

## Basic usage

### Diffing and Patching Files

```rust
use darkwing_diff::{diff, apply, DiffAlgorithm, CompressAlgorithm};

// Generate a patch
let before = b"original content";
let after = b"modified content";

let patch = diff(
    before,
    after,
    DiffAlgorithm::Rsync020,
    CompressAlgorithm::Zstd
)?;

// Apply the patch
let result = apply(before, &patch)?;

assert_eq!(&result, after);
```

### Working with ZIP Archives

```rust
use darkwing_diff::{diff, apply, DiffAlgorithm, CompressAlgorithm};

// Generate a patch set for all files in the ZIP archive
let patch_set = diff_zip(
    "before.zip".to_string(),
    "after.zip".to_string(),
    DiffAlgorithm::Rsync020,
    CompressAlgorithm::Zstd
)?;

// Apply the patch set to the ZIP archive
let result = apply_zip(
    "before.zip".to_string(),
    &patch_set,
    "applied.zip".to_string()
)?;
```

## Choosing algorithms

### Diff Algorithms

- `DiffAlgorithm::Rsync020`
  - Best for files that are mostly similar
  - Faster patch generation
  - Good for incremental updates

- `DiffAlgorithm::Bidiff1`
  - Better for files with major differences
  - May produce smaller patches
  - Slower patch generation

### Compression Algorithms

- `CompressAlgorithm::None`
  - No compression
  - Fastest performance
  - Good for debugging or already compressed data

- `CompressAlgorithm::Zstd`
  - High compression ratio (level 21)
  - Best for minimizing patch size
  - Slower due to compression overhead

## Advanced features

### Patch validation

All patches include MD5 hashes for validation:

```rust
use darkwing_diff::hash;

let patch = diff(before, after, diff_algo, compress_algo)?;

// Verify source file matches
assert_eq!(hash(before), patch.before_hash);

// After applying, verify result
let result = apply(before, &patch)?;

assert_eq!(hash(&result), patch.after_hash);
```

### ZIP Archive Operations

When working with ZIP archives, the library tracks different types of file operations:

```rust
pub enum Operation {
    Patch(Patch), // File was modified
    PutFile(Vec<u8>), // New file added
    DeleteFile, // File was removed
    FileStaysSame, // File is unchanged
}
```

### Serialization

Patches and patch sets can be serialized for storage or transmission:

```rust
// Serialize
let bytes = patch.to_bytes()?;
let patch_set_bytes = patch_set.to_bytes()?;

// Deserialize
let patch = Patch::from_bytes(&bytes)?;
let patch_set = PatchSet::from_bytes(&patch_set_bytes)?;
```

## Performance

TODO

## Testing

TODO

## MSRV Policy

TODO

## License

TODO

## Contributing

TODO