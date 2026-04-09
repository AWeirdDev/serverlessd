use std::{env, fs, path::PathBuf};

fn main() {
    let packages = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/lib"));
    let dirs = packages
        .read_dir()
        .expect("failed to read built-in js lib dir")
        .filter(|k| k.is_ok())
        .map(|item| {
            item.unwrap()
                .file_name()
                .into_string()
                .expect("failed to convert OsString to String")
        })
        .collect::<Vec<_>>();

    let mut arr = vec![];
    for dir in dirs {
        let path = PathBuf::from(format!("{}/lib/{}", env!("CARGO_MANIFEST_DIR"), dir));

        fs::read_dir(path)
            .expect("failed to read directory")
            .for_each(|entry| {
                let Some(entry) = entry.ok() else {
                    return;
                };

                let path = entry.path();

                if !path.is_file() {
                    return;
                }

                let Some(filename) = path.file_name() else {
                    return;
                };
                let filename = filename.to_string_lossy().into_owned();
                let name = filename.split_once(".").expect("failed to get filename before extension").0;

                arr.push(format!(
                    r#"({0:?}, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lib/{1}/", {2:?})))"#,
                    format!("{}:{}", dir, name),
                    dir,
                    filename
                ));
            });
    }

    fs::write(
        format!("{}/files.rs", env::var("OUT_DIR").unwrap()),
        format!(
            "#![rustfmt::skip]\npub(super) const FILES: [(&'static str, &'static str); {0}] = [{1}];\n",
            arr.len(),
            arr.join(", ")
        ),
    )
    .expect("failed to write to _scripts.rs");
}
