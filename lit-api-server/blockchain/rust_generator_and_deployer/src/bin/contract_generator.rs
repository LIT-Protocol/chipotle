use ethers::prelude::*;
use std::env;
use std::fs::{copy, create_dir_all, read_dir, write};
use std::path::Path;
use crate::common::parse_named_args;

fn main() {
    let args: Vec<String> = env::args().collect();
    let named = parse_named_args(&args);

    let bin = args.first().map(|s| s.as_str()).unwrap_or("generate_contracts");
    let usage = || {
        eprintln!(
            "Usage: {} --abifolder=<abifolder> --outputfolder=<outputfolder>",
            bin
        );
        eprintln!("  --abifolder    folder containing ABI files (e.g. ../lit-blockchain/abis/)");
        eprintln!("  --outputfolder folder to write generated .rs files (e.g. ../lit-blockchain/src/contracts/)");
        std::process::exit(1);
    };

    let input_folder = match named.get("abifolder") {
        Some(f) => f.trim_end_matches('/').to_string(),
        None => { eprintln!("Missing required arg: --abifolder"); usage(); unreachable!() }
    };
    let output_folder = match named.get("outputfolder") {
        Some(f) => f.trim_end_matches('/').to_string(),
        None => { eprintln!("Missing required arg: --outputfolder"); usage(); unreachable!() }
    };

    if !Path::new(&output_folder).exists() {
        create_dir_all(&output_folder).expect("Could not create output folder.");
    }
    process_folder(&input_folder, &output_folder);
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
