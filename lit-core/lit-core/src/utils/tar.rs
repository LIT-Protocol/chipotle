use crate::error::{Result, io_err};
use std::{
    fs::{File, create_dir_all},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn read_tar_gz_strip_components_file(
    input_file: impl AsRef<Path>, output_directory: impl AsRef<Path>, strip_components: usize,
) -> Result<()> {
    let input_file = input_file.as_ref();
    if !input_file.exists() {
        return Err(io_err(
            "Input file does not exist",
            Some(format!("Input file {} does not exist", input_file.display())),
        ));
    }
    let file = File::open(input_file)
        .map_err(|e| io_err(e, Some(format!("Unable to open file {}", input_file.display()))))?;
    read_tar_gz_strip_components(file, output_directory, strip_components)
}

pub fn read_tar_gz_strip_components(
    input: impl Read, output_directory: impl AsRef<Path>, strip_components: usize,
) -> Result<()> {
    let output_directory = output_directory.as_ref();
    if !output_directory.exists() {
        create_dir_all(output_directory).map_err(|e| {
            io_err(e, Some(format!("Unable to create directory {}", output_directory.display())))
        })?;
    }
    let decoder = flate2::read::GzDecoder::new(input);
    let mut archive = tar_file::Archive::new(decoder);

    for entry in archive
        .entries()
        .map_err(|e| io_err(e, Some("Unable to get entries from tar file".to_string())))?
    {
        let mut entry =
            entry.map_err(|e| io_err(e, Some("Unable to get entry from tar file".to_string())))?;
        let entry_type = entry.header().entry_type();

        let path = entry
            .path()
            .map_err(|e| io_err(e, Some("Unable to get path from tar entry".to_string())))?;

        let components = path.components().collect::<Vec<_>>();

        let stripped_path = if entry_type.is_dir() {
            path.components().skip(strip_components).collect::<PathBuf>()
        } else if components.len() <= strip_components {
            components.iter().collect::<PathBuf>()
        } else {
            components.iter().skip(strip_components).collect::<PathBuf>()
        };

        // if the path is empty, skip it
        if stripped_path.as_os_str().is_empty() {
            continue;
        }

        let output_path = output_directory.join(stripped_path);

        if let Some(parent) = output_path.parent() {
            create_dir_all(parent).map_err(|e| {
                io_err(e, Some(format!("Unable to create directory {}", parent.display())))
            })?;
        }

        entry.unpack(&output_path).map_err(|e| {
            io_err(e, Some(format!("Unable to unpack entry to {}", output_path.display())))
        })?;
    }

    Ok(())
}

pub fn read_tar_gz_file(
    input_file: impl AsRef<Path>, output_directory: impl AsRef<Path>,
) -> Result<()> {
    let input_file = input_file.as_ref();
    if !input_file.exists() {
        return Err(io_err(
            "Input file does not exist",
            Some(format!("Input file {} does not exist", input_file.display())),
        ));
    }
    let file = File::open(input_file)
        .map_err(|e| io_err(e, Some(format!("Unable to open file {}", input_file.display()))))?;
    read_tar_gz(file, output_directory)
}

pub fn read_tar_gz(input: impl Read, output_directory: impl AsRef<Path>) -> Result<()> {
    let output_directory = output_directory.as_ref();
    if !output_directory.exists() {
        create_dir_all(output_directory).map_err(|e| {
            io_err(e, Some(format!("Unable to create directory {}", output_directory.display())))
        })?;
    }
    let decoder = flate2::read::GzDecoder::new(input);
    let mut archive = tar_file::Archive::new(decoder);
    archive.unpack(output_directory).map_err(|e| {
        io_err(e, Some(format!("Unable to unpack tar into {}", output_directory.display())))
    })?;
    Ok(())
}

pub fn write_tar_gz_file(
    dir_to_pack: impl AsRef<Path>, output_file: impl AsRef<Path>,
) -> Result<()> {
    let output_file = output_file.as_ref();

    let parent = output_file
        .parent()
        .ok_or(io_err("Unable to get parent directory of input directory", None))?;
    if !parent.exists() {
        create_dir_all(parent).map_err(|e| {
            io_err(e, Some(format!("Unable to create directory {}", parent.display())))
        })?;
    }

    let file = File::create(output_file)
        .map_err(|e| io_err(e, Some(format!("Unable to create file {}", output_file.display()))))?;
    write_tar_gz(dir_to_pack, file)
}

pub fn write_tar_gz(dir_to_pack: impl AsRef<Path>, output: impl Write) -> Result<()> {
    let dir_to_pack = dir_to_pack.as_ref();
    if !dir_to_pack.is_dir() {
        return Err(io_err(
            "Input directory is not a directory",
            Some(format!("Input directory {} is not a directory", dir_to_pack.display())),
        ));
    }
    let encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
    let mut builder = tar_file::Builder::new(encoder);
    let name = dir_to_pack.file_name().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(""));
    builder.append_dir_all(name, dir_to_pack).map_err(|e| {
        io_err(e, Some(format!("Unable to append directory {} to tar file", dir_to_pack.display())))
    })?;
    builder.finish().map_err(|e| io_err(e, Some("Unable to finalize tar file".to_string())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tar_gz() {
        let dir = std::env::current_dir().unwrap().join("src");
        let output_file = std::env::current_dir().unwrap().join("test.tar.gz");
        write_tar_gz_file(&dir, &output_file).unwrap();
        assert!(output_file.exists());

        let test_dir = std::env::current_dir().unwrap().join("test_tar_gz");
        read_tar_gz_file(&output_file, &test_dir).unwrap();

        assert!(test_dir.exists());
        assert!(test_dir.join("src").exists());
        std::fs::remove_dir_all(test_dir).unwrap();
        std::fs::remove_file(output_file).unwrap();
    }
}
