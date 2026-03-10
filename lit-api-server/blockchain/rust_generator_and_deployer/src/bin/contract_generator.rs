use ethers::prelude::*;
use std::env;
use std::fs::{copy, create_dir_all, read_dir, write};
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <input_folder> <output_folder>",
            args.first().unwrap_or(&"generate_contracts".into())
        );
        eprintln!("  input_folder  - folder containing ABI files (e.g. ../lit-blockchain/abis/)");
        eprintln!(
            "  output_folder - folder to write generated .rs files (e.g. ../lit-blockchain/src/contracts/)"
        );
        std::process::exit(1);
    }
    let input_folder = args[1].trim_end_matches('/');
    let output_folder = args[2].trim_end_matches('/');

    if !Path::new(output_folder).exists() {
        create_dir_all(output_folder).expect("Could not create output folder.");
    }
    process_folder(input_folder, output_folder);
}

fn process_folder(input_folder: &str, output_folder: &str) {
    // process lit contracts
    let result = read_dir(input_folder);

    if result.is_err() {
        return;
    }

    let files = result.unwrap();
    for file in files.flatten() {
        if file.path().to_str().unwrap().contains("DiamondPattern") {
            continue;
        }
        println!("Processing file: {:?}", file.path());
        if file.file_type().unwrap().is_dir() {
            if file.path().to_str().unwrap().ends_with("Facets") {
                continue;
            }
            process_folder(file.path().to_str().unwrap(), output_folder);
            continue;
        }
        let file_path = file.path().canonicalize().unwrap();
        let file_name = file.file_name();


        let abi_source = file_path.to_str().unwrap();
        let result = Abigen::from_file(abi_source);

        match result {
            Ok(res) => {
                let result = res.add_derive("serde::Serialize");
                let result = result.unwrap();
                let result = result.add_derive("serde::Deserialize");
                let result = result.unwrap();

                if let Ok(bindings) = result.generate() {
                    let output_file_name =
                        format!("{}/{}.rs", output_folder, bindings.module_name());

                    // replace absolute path with relative
                    let as_str = bindings.to_string();
                    let as_str = as_str
                        .replace(abi_source, file.path().to_str().unwrap())
                        .replace(input_folder, ".");

                    write(output_file_name, as_str.clone())
                        .expect("Could not write generated file.");

                    let output_file_path =
                        format!("{}/{}", output_folder, file_name.to_str().unwrap());
                    copy(abi_source, output_file_path)
                        .expect("Could not copy abi file to the output folder.");
                }
            }
            Err(..) => {
                println!(
                    "Error generating ABI for {:?}:  {:?}",
                    file_path,
                    result.unwrap_err()
                );
            }
        }
    }
}
