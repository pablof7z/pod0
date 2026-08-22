use std::{fs, io::ErrorKind, path::Path};

use crate::{MediaError, Result};

pub(crate) fn publish_file(partial: &Path, output: &Path, overwrite: bool) -> Result<()> {
    if overwrite {
        fs::rename(partial, output)?;
        return Ok(());
    }

    match fs::hard_link(partial, output) {
        Ok(()) => {
            fs::remove_file(partial)?;
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            Err(MediaError::OutputExists(output.to_owned()))
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::publish_file;
    use crate::MediaError;

    #[cfg(unix)]
    #[test]
    fn no_replace_preserves_conflicting_destination_entry() {
        use std::os::unix::fs::symlink;

        let directory = test_directory();
        let partial = directory.join("partial.wav");
        let output = directory.join("output.wav");
        fs::write(&partial, b"new clip").expect("write partial");
        symlink("missing-target", &output).expect("create conflicting destination");

        let error =
            publish_file(&partial, &output, false).expect_err("reject destination conflict");

        assert!(matches!(error, MediaError::OutputExists(path) if path == output));
        assert!(
            fs::symlink_metadata(&output)
                .expect("inspect destination")
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&partial).expect("partial remains"), b"new clip");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    fn test_directory() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("publish-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test directory");
        path
    }
}
