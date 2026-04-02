use std::{env, fs, path::Path};

fn main() {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/intrinsics"));

    let arr = fs::read_dir(path)
        .expect("failed to read directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();

            if !path.is_file() {
                return None;
            }

            let filename = path.file_name()?.to_string_lossy().into_owned();
            let name = filename.split_once(".").expect("failed to get filename before extension").0;

            Some(format!(
                r#"({0:?}, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/intrinsics/", {1:?})))"#,
                name,
                filename
            ))
        })
        .collect::<Vec<_>>();

    fs::write(
        format!("{}/files.rs", env::var("OUT_DIR").unwrap()),
        format!(
            "#[rustfmt::skip]\npub(super) const FILES: [(&'static str, &'static str); {0}] = [{1}];\n",
            arr.len(),
            arr.join(", ")
        ),
    )
    .expect("failed to write to _scripts.rs");
}
