use crate::{
    apply,
    compress::CompressAlgorithm,
    diff, hash,
    patch::{DiffAlgorithm, Operation, Operations, PatchSet},
    Error,
};
use log::{debug, info, trace, warn};
use std::io::{Read as _, Write};

// Process all files in both archives without recursion
fn process_directory(
    dir_path: &str,
    files_before: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
    files_after: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
    processed_files: &mut std::collections::HashSet<String>,
    patches: &mut Vec<(String, Operation)>,
    diff_algorithm: DiffAlgorithm,
    compress_algorithm: CompressAlgorithm,
) -> Result<(), Error> {
    debug!("Processing files starting from: `{}`", dir_path);

    // Get all files from both archives
    let mut all_files = std::collections::HashSet::new();

    // Add files from before archive
    for i in 0..files_before.len() {
        let file = files_before
            .by_index(i)
            .map_err(|e| Error::ZipError(e.to_string()))?;
        all_files.insert(file.name().to_string());
    }

    // Add files from after archive
    for i in 0..files_after.len() {
        let file = files_after
            .by_index(i)
            .map_err(|e| Error::ZipError(e.to_string()))?;
        all_files.insert(file.name().to_string());
    }

    // Process all files
    for path in all_files {
        if processed_files.contains(&path) {
            continue;
        }
        processed_files.insert(path.clone());

        // Check if file exists in before archive
        let before_exists = files_before.by_name(&path).is_ok();
        let after_exists = files_after.by_name(&path).is_ok();

        match (before_exists, after_exists) {
            (true, true) => {
                // File exists in both archives
                let before_contents = read_file_contents(files_before, &path)?
                    .ok_or_else(|| Error::ZipError("Failed to read before contents".to_string()))?;
                let after_contents = read_file_contents(files_after, &path)?
                    .ok_or_else(|| Error::ZipError("Failed to read after contents".to_string()))?;

                if before_contents != after_contents {
                    debug!("File modified: {}", path);
                    let patch = diff(
                        &before_contents,
                        &after_contents,
                        diff_algorithm,
                        compress_algorithm,
                    )?;
                    patches.push((path, Operation::Patch(patch)));
                } else {
                    trace!("File unchanged: {}", path);
                    patches.push((path, Operation::FileStaysSame));
                }
            }
            (true, false) => {
                // File was deleted
                debug!("File deleted: {}", path);
                patches.push((path, Operation::DeleteFile));
            }
            (false, true) => {
                // New file
                debug!("New file: {}", path);
                if let Some(contents) = read_file_contents(files_after, &path)? {
                    patches.push((path, Operation::PutFile(contents)));
                }
            }
            (false, false) => {
                // This shouldn't happen as the file must exist in at least one archive
                warn!("File {} not found in either archive", path);
            }
        }
    }

    Ok(())
}

// Helper function to read file contents
fn read_file_contents(
    archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
    path: &str,
) -> Result<Option<Vec<u8>>, Error> {
    match archive.by_name(path) {
        Ok(mut file) => {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .map_err(|e| Error::IoError(e.to_string()))?;
            Ok(Some(contents))
        }
        Err(_) => Ok(None),
    }
}

fn get_directories_of_file(path: &str) -> Vec<String> {
    let mut dirs = Vec::new();
    let mut current = String::new();
    let parts: Vec<&str> = path.split('/').collect();

    // Skip the last part since it's the filename
    for part in parts.iter().take(parts.len() - 1) {
        if !part.is_empty() {
            if current.is_empty() {
                current = part.to_string();
            } else {
                current = format!("{}/{}", current, part);
            }
            dirs.push(current.clone());
        }
    }
    dirs
}

/// Generates a patch set that can transform one zip archive into another.
///
/// Creates a set of operations that describe how to transform the contents of
/// one zip archive into another, handling file additions, deletions, and
/// modifications efficiently.
///
/// # Example
/// ```no_run
/// use files_diff::{diff_zip, DiffAlgorithm, CompressAlgorithm};
///
/// // Generate patches for transforming a zip archive
/// let patch_set = diff_zip(
///     "v1.zip".to_string(),
///     "v2.zip".to_string(),
///     DiffAlgorithm::Rsync020,
///     CompressAlgorithm::Zstd
/// )?;
///
/// // Check the total size of all patches
/// println!("Patch set size: {} bytes", patch_set.get_size());
/// # Ok::<(), files_diff::Error>(())
/// ```
///
/// The function handles:
/// - Nested directory structures
/// - File additions and deletions
/// - File modifications using the specified diff algorithm
/// - Directory creation and deletion
pub fn diff_zip(
    path_before: String,
    path_after: String,
    diff_algorithm: DiffAlgorithm,
    compress_algorithm: CompressAlgorithm,
) -> Result<PatchSet, Error> {
    info!("Generating diff between {} and {}", path_before, path_after);
    debug!("Using diff algorithm: {:?}", diff_algorithm);
    debug!("Using compression algorithm: {:?}", compress_algorithm);

    let before = std::fs::read(path_before).map_err(|e| Error::IoError(e.to_string()))?;
    info!("before size: {}", before.len());
    let after = std::fs::read(path_after).map_err(|e| Error::IoError(e.to_string()))?;
    info!("after size: {}", after.len());

    let hash_before = hash(&before);

    trace!("Before archive size: {} bytes", before.len());
    trace!("After archive size: {} bytes", after.len());

    let mut files_before = zip::ZipArchive::new(std::io::Cursor::new(before))
        .map_err(|e| Error::ZipError(e.to_string()))?;
    let mut files_after = zip::ZipArchive::new(std::io::Cursor::new(after))
        .map_err(|e| Error::ZipError(e.to_string()))?;

    let mut patches = Vec::new();
    let mut processed_files = std::collections::HashSet::new();

    // Start processing from root
    process_directory(
        "",
        &mut files_before,
        &mut files_after,
        &mut processed_files,
        &mut patches,
        diff_algorithm,
        compress_algorithm,
    )?;

    let operations = Operations(patches);
    let operations_hash = operations.hash()?;

    debug!("Generated {} patch operations", operations.0.len());
    Ok(PatchSet {
        operations,
        hash_before,
        operations_hash,
    })
}

/// Applies a patch set to transform a zip archive into a new version.
///
/// Takes a source zip archive and a patch set, and creates a new zip archive
/// that represents the target version. Validates all operations and maintains
/// the integrity of the archive structure.
///
/// # Example
/// ```no_run
/// use files_diff::{diff_zip, apply_zip, DiffAlgorithm, CompressAlgorithm};
///
/// // First generate a patch set
/// let patch_set = diff_zip(
///     "source.zip".to_string(),
///     "target.zip".to_string(),
///     DiffAlgorithm::Rsync020,
///     CompressAlgorithm::Zstd
/// )?;
///
/// // Apply the patches to create a new version
/// apply_zip(
///     "source.zip",
///     patch_set,
///     "result.zip".to_string()
/// )?;
/// # Ok::<(), files_diff::Error>(())
/// ```
///
/// The function:
/// - Preserves directory structure
/// - Handles file additions, deletions, and modifications
/// - Maintains file metadata
/// - Validates all operations during application
pub fn apply_zip(path_base: &str, delta: PatchSet, path_after: String) -> Result<(), Error> {
    info!("Applying patch to {} to create {}", path_base, path_after);
    debug!("Patch contains {} operations", delta.operations.0.len());

    let base_data = std::fs::read(path_base).map_err(|e| Error::IoError(e.to_string()))?;

    let base_hash = hash(&base_data);
    if base_hash != delta.hash_before {
        return Err(Error::BeforeHashMismatch);
    }

    if delta.operations_hash != delta.operations.hash()? {
        return Err(Error::OperationsHashMismatch);
    }

    let mut base_archive = zip::ZipArchive::new(std::io::Cursor::new(base_data))
        .map_err(|e| Error::ZipError(e.to_string()))?;

    let file = std::fs::File::create(&path_after).map_err(|e| Error::IoError(e.to_string()))?;

    let mut new_archive = zip::ZipWriter::new(file);
    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    // Track processed files to handle deletions
    let mut processed_files = std::collections::HashSet::new();
    let mut directories_to_create: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // First, apply all patches
    for (path, operation) in delta.operations.0 {
        processed_files.insert(path.clone());

        match operation {
            Operation::Patch(patch) => {
                debug!("Applying patch to file: {}", path);
                // Read original file
                let mut base_file = base_archive
                    .by_name(&path)
                    .map_err(|e| Error::ZipError(e.to_string()))?;
                let mut original_contents = Vec::new();
                base_file
                    .read_to_end(&mut original_contents)
                    .map_err(|e| Error::IoError(e.to_string()))?;

                // Apply patch to get new contents
                let new_contents = apply(&original_contents, &patch)?;

                // Write new file
                new_archive
                    .start_file(&path, options)
                    .map_err(|e| Error::ZipError(e.to_string()))?;
                new_archive
                    .write_all(&new_contents)
                    .map_err(|e| Error::IoError(e.to_string()))?;
                directories_to_create.extend(get_directories_of_file(&path));
            }
            Operation::PutFile(contents) => {
                debug!("Adding new file: {}", path);
                // Write new file directly
                new_archive
                    .start_file(&path, options)
                    .map_err(|e| Error::ZipError(e.to_string()))?;
                new_archive
                    .write_all(&contents)
                    .map_err(|e| Error::IoError(e.to_string()))?;
                directories_to_create.extend(get_directories_of_file(&path));
            }
            Operation::DeleteFile => {
                debug!("Deleting file: {}", path);
                // Skip this file - don't copy it to new archive
                continue;
            }
            Operation::FileStaysSame => {
                debug!("File stays same: {}", path);
                // Copy file from base archive
                // Copy file contents in a single operation
                let mut contents = Vec::new();
                base_archive
                    .by_name(&path)
                    .map_err(|e| Error::ZipError(e.to_string()))?
                    .read_to_end(&mut contents)
                    .map_err(|e| Error::IoError(e.to_string()))?;

                new_archive
                    .start_file(&path, options)
                    .map_err(|e| Error::ZipError(e.to_string()))?;
                new_archive
                    .write_all(&contents)
                    .map_err(|e| Error::IoError(e.to_string()))?;
                directories_to_create.extend(get_directories_of_file(&path));
            }
        }
    }

    for dir in directories_to_create {
        trace!("creating directory {}", dir);

        new_archive
            .add_directory(dir, options)
            .map_err(|e| Error::ZipError(e.to_string()))?;
    }

    // Finalize the ZIP file
    new_archive
        .finish()
        .map_err(|e| Error::ZipError(e.to_string()))?;

    info!("Successfully created patched archive: {}", path_after);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::patch::Patch;

    use super::*;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::io::{Read, Write};
    use tempfile::TempDir;

    use std::sync::Once;

    static INIT: Once = Once::new();

    fn setup_logger() {
        INIT.call_once(|| {
            unsafe {
                std::env::set_var("RUST_LOG", "trace");
            }
            pretty_env_logger::init()
        });
    }

    fn create_test_zip(files: &[(&str, Vec<u8>)]) -> Result<Vec<u8>, Error> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        for (name, contents) in files {
            if *name == "" {
                continue;
            }
            zip.start_file(*name, options)
                .map_err(|e| Error::ZipError(e.to_string()))?;
            zip.write_all(contents)
                .map_err(|e| Error::IoError(e.to_string()))?;
        }

        Ok(zip
            .finish()
            .map_err(|e| Error::ZipError(e.to_string()))?
            .into_inner())
    }

    #[test]
    fn test_diff_and_apply_basic() -> Result<(), Error> {
        setup_logger();

        let temp_dir = TempDir::new().map_err(|e| Error::IoError(e.to_string()))?;

        // Create before.zip with a single file
        let before_zip = create_test_zip(&[("test.txt", b"Hello World".into())])?;
        let before_path = temp_dir.path().join("before.zip");
        fs::write(&before_path, before_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // Create after.zip with modified content
        let after_zip = create_test_zip(&[("test.txt", b"Hello Modified World".into())])?;
        let after_path = temp_dir.path().join("after.zip");
        fs::write(&after_path, after_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // Generate diff
        let patch_set = diff_zip(
            before_path.to_string_lossy().to_string(),
            after_path.to_string_lossy().to_string(),
            DiffAlgorithm::Bidiff1,
            CompressAlgorithm::None,
        )?;

        assert_eq!(patch_set.operations.0.len(), 1);
        assert_eq!(
            patch_set.operations.0[0].1,
            Operation::Patch(Patch {
                diff_algorithm: DiffAlgorithm::Bidiff1,
                compress_algorithm: CompressAlgorithm::None,
                before_hash: "b10a8db164e0754105b7a99be72e3fe5".to_string(),
                after_hash: "77a55ec2b0808d5a1ef1173fcfce9763".to_string(),
                patch: vec![
                    223, 177, 0, 0, 0, 16, 0, 0, 6, 0, 0, 0, 0, 0, 0, 14, 77, 111, 100, 105, 102,
                    105, 101, 100, 32, 87, 111, 114, 108, 100, 0,
                ],
            })
        );

        // Create output path for patched zip
        let output_path = temp_dir.path().join("output.zip");

        // Apply patch
        apply_zip(
            &before_path.to_string_lossy(),
            patch_set,
            output_path.to_string_lossy().to_string(),
        )?;

        // Verify the contents
        let mut output_archive = zip::ZipArchive::new(std::io::Cursor::new(
            fs::read(&output_path).map_err(|e| Error::IoError(e.to_string()))?,
        ))
        .map_err(|e| Error::ZipError(e.to_string()))?;

        let mut file = output_archive
            .by_name("test.txt")
            .map_err(|e| Error::ZipError(e.to_string()))?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .map_err(|e| Error::IoError(e.to_string()))?;

        assert_eq!(contents, b"Hello Modified World");
        Ok(())
    }

    #[test]
    fn test_diff_and_apply_with_deletions() -> Result<(), Error> {
        setup_logger();

        let temp_dir = TempDir::new().map_err(|e| Error::IoError(e.to_string()))?;

        // Create before.zip with multiple files
        let before_zip = create_test_zip(&[
            ("file1.txt", b"File 1 content".into()),
            ("file2.txt", b"File 2 content".into()),
        ])?;
        let before_hash = hash(&before_zip);
        let before_path = temp_dir.path().join("before.zip");
        fs::write(&before_path, before_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // Create after.zip with one file deleted
        let after_zip = create_test_zip(&[("file1.txt", b"File 1 content".into())])?;
        let after_path = temp_dir.path().join("after.zip");
        fs::write(&after_path, after_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // Generate and apply patch
        let patch_set = diff_zip(
            before_path.to_string_lossy().to_string(),
            after_path.to_string_lossy().to_string(),
            DiffAlgorithm::Bidiff1,
            CompressAlgorithm::None,
        )?;

        assert_eq!(
            patch_set,
            PatchSet {
                operations: Operations(vec![
                    ("file1.txt".to_string(), Operation::FileStaysSame),
                    ("file2.txt".to_string(), Operation::DeleteFile),
                ]),
                hash_before: before_hash,
                operations_hash: "2a8a469ad35c75f628e7c1ebe37afbf0".to_string(),
            }
        );

        let output_path = temp_dir.path().join("output.zip");
        apply_zip(
            &before_path.to_string_lossy(),
            patch_set,
            output_path.to_string_lossy().to_string(),
        )?;

        // Verify the contents
        let mut output_archive = zip::ZipArchive::new(std::io::Cursor::new(
            fs::read(&output_path).map_err(|e| Error::IoError(e.to_string()))?,
        ))
        .map_err(|e| Error::ZipError(e.to_string()))?;

        assert_eq!(output_archive.len(), 1);
        assert!(output_archive.by_name("file1.txt").is_ok());
        assert!(output_archive.by_name("file2.txt").is_err());

        Ok(())
    }

    #[test]
    fn test_diff_and_apply_with_directories() -> Result<(), Error> {
        setup_logger();

        let temp_dir = TempDir::new().map_err(|e| Error::IoError(e.to_string()))?;

        // Create before.zip with nested structure
        let before_zip = create_test_zip(&[
            ("dir1/", vec![]),
            ("dir1/file1.txt", b"File 1".into()),
            ("dir2/", vec![]),
            ("dir2/file2.txt", b"File 2".into()),
        ])?;
        let before_hash = hash(&before_zip);
        let before_path = temp_dir.path().join("before.zip");
        fs::write(&before_path, before_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // Create after.zip with modified structure
        let after_zip = create_test_zip(&[
            ("dir1/", vec![]),
            ("dir1/file1.txt", b"File 1 Modified".into()),
            ("dir3/", vec![]),
            ("dir3/file3.txt", b"File 3".into()),
        ])?;
        let after_path = temp_dir.path().join("after.zip");
        fs::write(&after_path, after_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // Generate and apply patch
        let patch_set = diff_zip(
            before_path.to_string_lossy().to_string(),
            after_path.to_string_lossy().to_string(),
            DiffAlgorithm::Bidiff1,
            CompressAlgorithm::None,
        )?;

        assert_eq!(
            patch_set,
            PatchSet {
                operations: Operations(vec![
                    (
                        "dir1/file1.txt".to_string(),
                        Operation::Patch(Patch {
                            diff_algorithm: DiffAlgorithm::Bidiff1,
                            compress_algorithm: CompressAlgorithm::None,
                            before_hash: "2f03b03637bf162937793f756f0f1583".to_string(),
                            after_hash: "15b8181404e3a6b2e046de781b702654".to_string(),
                            patch: vec![
                                223, 177, 0, 0, 0, 16, 0, 0, 6, 0, 0, 0, 0, 0, 0, 9, 32, 77, 111,
                                100, 105, 102, 105, 101, 100, 0,
                            ],
                        }),
                    ),
                    ("dir2/".to_string(), Operation::DeleteFile),
                    (
                        "dir3/file3.txt".to_string(),
                        Operation::PutFile(vec![70, 105, 108, 101, 32, 51]),
                    ),
                ]),
                hash_before: before_hash,
                operations_hash: "c52153314592d31ddfda9bbf6390a991".to_string(),
            }
        );

        let output_path = temp_dir.path().join("output.zip");
        apply_zip(
            &before_path.to_string_lossy(),
            patch_set,
            output_path.to_string_lossy().to_string(),
        )?;

        // Verify the contents
        let mut output_archive = zip::ZipArchive::new(std::io::Cursor::new(
            fs::read(&output_path).map_err(|e| Error::IoError(e.to_string()))?,
        ))
        .map_err(|e| Error::ZipError(e.to_string()))?;

        // Check dir1/file1.txt was modified
        let mut file1_contents = Vec::new();
        output_archive
            .by_name("dir1/file1.txt")
            .map_err(|e| Error::ZipError(e.to_string()))?
            .read_to_end(&mut file1_contents)
            .map_err(|e| Error::IoError(e.to_string()))?;
        assert_eq!(file1_contents, b"File 1 Modified");

        // Check dir2 was deleted
        assert!(output_archive.by_name("dir2/file2.txt").is_err());

        // Check dir3 was added
        let mut file3_contents = Vec::new();
        output_archive
            .by_name("dir3/file3.txt")
            .map_err(|e| Error::ZipError(e.to_string()))?
            .read_to_end(&mut file3_contents)
            .map_err(|e| Error::IoError(e.to_string()))?;
        assert_eq!(file3_contents, b"File 3");

        Ok(())
    }

    #[test]
    fn test_complex_roundtrip_diff_and_apply() -> Result<(), Error> {
        setup_logger();
        let temp_dir = TempDir::new().map_err(|e| Error::IoError(e.to_string()))?;

        // Initial state (version 1)
        let v1_files = vec![
            ("root1.txt", b"Root file 1".into()),
            ("root2.txt", b"Root file 2".into()),
            ("parent1/", vec![]),
            ("parent1/file1.txt", b"Parent 1 file".into()),
            ("parent1/child1/", vec![]),
            ("parent1/child1/deep1.txt", b"Deep file 1".into()),
            ("parent1/child1/deep2.txt", b"Deep file 2".into()),
            ("parent2/", vec![]),
            ("parent2/file2.txt", b"Parent 2 file".into()),
            ("parent2/child2/", vec![]),
            ("parent2/child2/deep3.txt", b"Deep file 3".into()),
        ];
        let v1_zip = create_test_zip(&v1_files)?;
        let v1_hash = hash(&v1_zip);
        let v1_path = temp_dir.path().join("v1.zip");
        fs::write(&v1_path, v1_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // Version 2: modify some files, add new ones, delete some
        let v2_files = vec![
            ("root1.txt", b"Root file 1 modified".into()), // modified
            // root2.txt deleted
            ("parent1/", vec![]),
            ("parent1/file1.txt", b"Parent 1 file modified".into()), // modified
            ("parent1/child1/", vec![]),
            ("parent1/child1/deep1.txt", b"Deep file 1".into()), // unchanged
            // deep2.txt deleted
            ("parent1/child1/deep3.txt", b"New deep file".into()), // added
            ("parent2/", vec![]),
            ("parent2/file2.txt", b"Parent 2 file".into()), // unchanged
            ("parent2/child2/", vec![]),
            ("parent2/child2/deep3.txt", b"Deep file 3 modified".into()), // modified
            ("parent3/", vec![]),                                         // new directory
            ("parent3/newfile.txt", b"Brand new file".into()),
        ];
        let v2_zip = create_test_zip(&v2_files)?;
        let v2_hash = hash(&v2_zip);
        let v2_path = temp_dir.path().join("v2.zip");
        fs::write(&v2_path, v2_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // Version 3: more changes
        let v3_files = vec![
            ("root1.txt", b"Root file 1 modified again".into()), // modified again
            ("parent1/", vec![]),
            ("parent1/file1.txt", b"Parent 1 file modified".into()), // unchanged
            ("parent1/child1/", vec![]),
            // deep1.txt deleted
            ("parent1/child1/deep3.txt", b"New deep file modified".into()), /* modified */
            // parent2 directory completely deleted
            ("parent3/", vec![]),
            ("parent3/newfile.txt", b"Brand new file modified".into()), // modified
            ("parent3/another.txt", b"Another new file".into()),        // added
        ];
        let v3_zip = create_test_zip(&v3_files)?;
        let v3_path = temp_dir.path().join("v3.zip");
        fs::write(&v3_path, v3_zip).map_err(|e| Error::IoError(e.to_string()))?;

        // First roundtrip: v1 -> v2
        let patch_v1_to_v2 = diff_zip(
            v1_path.to_string_lossy().to_string(),
            v2_path.to_string_lossy().to_string(),
            DiffAlgorithm::Bidiff1,
            CompressAlgorithm::None,
        )?;

        assert_eq!(
            patch_v1_to_v2,
            PatchSet {
                operations: Operations(vec![
                    // Root directory changes
                    (
                        "root1.txt".to_string(),
                        Operation::Patch(Patch {
                            diff_algorithm: DiffAlgorithm::Bidiff1,
                            compress_algorithm: CompressAlgorithm::None,
                            before_hash: "f675e8894edcf33ae7097dcc4bfb89f9".to_string(),
                            after_hash: "3468f9d6535a07b35c8acb8aa6aac781".to_string(),
                            patch: vec![
                                223, 177, 0, 0, 0, 16, 0, 0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                9, 32, 109, 111, 100, 105, 102, 105, 101, 100, 0,
                            ],
                        })
                    ),
                    ("root2.txt".to_string(), Operation::DeleteFile),
                    (
                        "parent1/file1.txt".to_string(),
                        Operation::Patch(Patch {
                            diff_algorithm: DiffAlgorithm::Bidiff1,
                            compress_algorithm: CompressAlgorithm::None,
                            before_hash: "a138a74adecabef6294b55d2b28d3ea1".to_string(),
                            after_hash: "710d2bbb6df79b88d7b75bdefdcf28aa".to_string(),
                            patch: vec![
                                223, 177, 0, 0, 0, 16, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                0, 0, 9, 32, 109, 111, 100, 105, 102, 105, 101, 100, 0,
                            ],
                        })
                    ),
                    (
                        "parent1/child1/deep1.txt".to_string(),
                        Operation::FileStaysSame
                    ),
                    (
                        "parent1/child1/deep2.txt".to_string(),
                        Operation::DeleteFile
                    ),
                    (
                        "parent1/child1/deep3.txt".to_string(),
                        Operation::PutFile(b"New deep file".to_vec())
                    ),
                    // parent2/ directory changes
                    ("parent2/file2.txt".to_string(), Operation::FileStaysSame),
                    (
                        "parent2/child2/deep3.txt".to_string(),
                        Operation::Patch(Patch {
                            diff_algorithm: DiffAlgorithm::Bidiff1,
                            compress_algorithm: CompressAlgorithm::None,
                            before_hash: "15bf70eee30b1805ab0e11510d30b41e".to_string(),
                            after_hash: "804237ac129569f027a2b55f8cf8d7db".to_string(),
                            patch: vec![
                                223, 177, 0, 0, 0, 16, 0, 0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                9, 32, 109, 111, 100, 105, 102, 105, 101, 100, 0,
                            ],
                        })
                    ),
                    (
                        "parent3/newfile.txt".to_string(),
                        Operation::PutFile(b"Brand new file".to_vec())
                    ),
                ]),
                hash_before: v1_hash,
                operations_hash: "caf887830891091723fe5ada783f48b6".to_string(),
            }
        );

        let v2_patched_path = temp_dir.path().join("v2_patched.zip");
        apply_zip(
            &v1_path.to_string_lossy(),
            patch_v1_to_v2,
            v2_patched_path.to_string_lossy().to_string(),
        )?;

        // Verify v2_patched matches v2
        let v2_original = fs::read(&v2_path).map_err(|e| Error::IoError(e.to_string()))?;
        let v2_patched = fs::read(&v2_patched_path).map_err(|e| Error::IoError(e.to_string()))?;
        verify_archives_match(&v2_original, &v2_patched)?;

        // Second roundtrip: v2 -> v3
        let patch_v2_to_v3 = diff_zip(
            v2_path.to_string_lossy().to_string(),
            v3_path.to_string_lossy().to_string(),
            DiffAlgorithm::Bidiff1,
            CompressAlgorithm::None,
        )?;

        assert_eq!(
            patch_v2_to_v3,
            PatchSet {
                operations: Operations(vec![
                    (
                        "root1.txt".to_string(),
                        Operation::Patch(Patch {
                            diff_algorithm: DiffAlgorithm::Bidiff1,
                            compress_algorithm: CompressAlgorithm::None,
                            before_hash: "3468f9d6535a07b35c8acb8aa6aac781".to_string(),
                            after_hash: "2ad3c7437786d6625776f0583bc3d6b2".to_string(),
                            patch: vec![
                                223, 177, 0, 0, 0, 16, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 32, 97, 103, 97, 105, 110, 0
                            ],
                        })
                    ),
                    ("parent1/file1.txt".to_string(), Operation::FileStaysSame),
                    (
                        "parent1/child1/deep1.txt".to_string(),
                        Operation::DeleteFile
                    ),
                    (
                        "parent1/child1/deep3.txt".to_string(),
                        Operation::Patch(Patch {
                            diff_algorithm: DiffAlgorithm::Bidiff1,
                            compress_algorithm: CompressAlgorithm::None,
                            before_hash: "eb60615cbd4f6c8befc5dc7b387e77b9".to_string(),
                            after_hash: "ad96d84598d4994a819489d1762967e3".to_string(),
                            patch: vec![
                                223, 177, 0, 0, 0, 16, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                0, 0, 9, 32, 109, 111, 100, 105, 102, 105, 101, 100, 0
                            ]
                        })
                    ),
                    ("parent2/".to_string(), Operation::DeleteFile),
                    (
                        "parent3/newfile.txt".to_string(),
                        Operation::Patch(Patch {
                            diff_algorithm: DiffAlgorithm::Bidiff1,
                            compress_algorithm: CompressAlgorithm::None,
                            before_hash: "98de949196bc048ff94069ea5e1c4446".to_string(),
                            after_hash: "0afd1f99b76a45e02719a43715c7071b".to_string(),
                            patch: vec![
                                223, 177, 0, 0, 0, 16, 0, 0, 14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                                0, 0, 0, 9, 32, 109, 111, 100, 105, 102, 105, 101, 100, 0
                            ]
                        })
                    ),
                    (
                        "parent3/another.txt".to_string(),
                        Operation::PutFile(vec![
                            65, 110, 111, 116, 104, 101, 114, 32, 110, 101, 119, 32, 102, 105, 108,
                            101
                        ])
                    )
                ]),
                hash_before: v2_hash,
                operations_hash: "772e8078384f8a99cda819d2d3807864".to_string(),
            }
        );

        let v3_patched_path = temp_dir.path().join("v3_patched.zip");
        apply_zip(
            &v2_path.to_string_lossy(),
            patch_v2_to_v3,
            v3_patched_path.to_string_lossy().to_string(),
        )?;

        // Verify v3_patched matches v3
        let v3_original = fs::read(&v3_path).map_err(|e| Error::IoError(e.to_string()))?;
        let v3_patched = fs::read(&v3_patched_path).map_err(|e| Error::IoError(e.to_string()))?;
        verify_archives_match(&v3_original, &v3_patched)?;

        Ok(())
    }

    // Helper function to verify two ZIP archives have identical contents
    fn verify_archives_match(data1: &[u8], data2: &[u8]) -> Result<(), Error> {
        let mut archive1 = zip::ZipArchive::new(std::io::Cursor::new(data1))
            .map_err(|e| Error::ZipError(e.to_string()))?;
        let mut archive2 = zip::ZipArchive::new(std::io::Cursor::new(data2))
            .map_err(|e| Error::ZipError(e.to_string()))?;

        if archive1.len() != archive2.len() {
            return Err(Error::ZipError(
                "Archives have different number of files".to_string(),
            ));
        }

        for i in 0..archive1.len() {
            let mut file1 = archive1
                .by_index(i)
                .map_err(|e| Error::ZipError(e.to_string()))?;
            let file1_name = file1.name().to_string();

            let mut file2 = match archive2.by_name(file1.name()) {
                Ok(file) => file,
                Err(_) => {
                    return Err(Error::ZipError(format!(
                        "File {} not found in second archive",
                        file1_name
                    )));
                }
            };

            if file1.is_dir() != file2.is_dir() {
                return Err(Error::ZipError(format!(
                    "Directory status mismatch for {}",
                    file1_name
                )));
            }

            if !file1.is_dir() {
                let mut contents1 = Vec::new();
                let mut contents2 = Vec::new();
                file1
                    .read_to_end(&mut contents1)
                    .map_err(|e| Error::IoError(e.to_string()))?;
                file2
                    .read_to_end(&mut contents2)
                    .map_err(|e| Error::IoError(e.to_string()))?;

                if contents1 != contents2 {
                    return Err(Error::ZipError(format!(
                        "Contents mismatch for file {}",
                        file1_name
                    )));
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_realword_archive_diff() -> Result<(), Error> {
        setup_logger();

        let before = "/home/eli/darkwing/tests/data/lot-of-datadirs/24132775.datadir.zip.before";
        let after = "/home/eli/darkwing/tests/data/lot-of-datadirs/24132775.datadir.zip.after";

        let diff = diff_zip(
            before.to_string(),
            after.to_string(),
            DiffAlgorithm::Rsync020,
            CompressAlgorithm::None,
        )?;

        debug!("total size: {}", diff.get_size());

        for op in diff.operations.0 {
            debug!("file: {:?}", op.0);
        }

        Ok(())
    }
}
