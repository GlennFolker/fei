pub use proc_macro2;
pub use syn;
pub use toml;
pub use quote;

pub mod prelude {
    pub use proc_macro2;
    pub use syn;
    pub use toml;
    pub use quote;
}

use std::{
    env,
    fs,
    path::PathBuf,
    sync::OnceLock,
};
use syn::Path;
use toml::{
    Table, Value,
};

static FEI_MANIFEST: OnceLock<Table> = OnceLock::new();

pub fn get_manifest() -> &'static Table {
    FEI_MANIFEST.get_or_init(|| {
        env::var_os("CARGO_MANIFEST_DIR")
            .map(|path| {
                let mut file = PathBuf::from(&path);
                file.push("Cargo.toml");

                assert!(file.exists(), "`Cargo.toml` doesn't exist in {}", path.to_string_lossy());

                let file = fs::read_to_string(file).expect("couldn't read {file}");
                file.parse::<Table>().unwrap_or_else(|_| panic!("invalid `Cargo.toml` found in {}", path.to_string_lossy()))
            })
            .expect("`CARGO_MANIFEST_DIR` env var not set")
    })
}

pub fn module(name: &str) -> syn::Result<Option<Path>> {
    let search = |deps: &Table| -> syn::Result<Option<Path>> {
        if let Some(Value::String(..)) = deps.get(name) {
            // `fei-*` is imported by `fei-* = "<version>"`; returning `fei-*`.
            Some(syn::parse_str(name)).transpose()
        } else if let Some(Value::String(..)) = deps.get("fei") {
            // `fei-*` is imported by `fei = "<version>"`; returning `fei::*`.
            let mut fei = syn::parse_str::<Path>("fei")?;
            fei.segments.push(syn::parse_str(name.strip_prefix("fei-").unwrap_or(name))?);
            Ok(Some(fei))
        } else {
            for (alias, dep) in deps {
                if let Value::Table(spec) = dep {
                    match spec.get("package") {
                        Some(Value::String(actual)) => {
                            if actual == name {
                                // `fei-*` is imported by `<x> = { package = "fei-*" }`; returning `<x>`.
                                return Some(syn::parse_str(alias)).transpose();
                            } else if actual == "fei" {
                                // `fei-*` is imported by `<x> = { package = "fei" }`; returning `<x>::*`.
                                let mut fei = syn::parse_str::<Path>(alias)?;
                                fei.segments.push(syn::parse_str(name.strip_prefix("fei-").unwrap_or(name))?);
                                return Ok(Some(fei));
                            }
                        },
                        // `fei-*` is imported by `fei-* = {}`; returning `fei-*`.
                        None if alias == name => return Some(syn::parse_str(name)).transpose(),
                        _ => {},
                    }
                }
            }

            Ok(None)
        }
    };

    let manifest = get_manifest();
    manifest
        .get("dependencies")
        .and_then(|deps| match deps {
            Value::Table(deps) => search(deps).transpose(),
            _ => unreachable!("`dependencies` is always a table"),
        })
        .or_else(|| manifest
            .get("dev-dependencies")
            .and_then(|deps| match deps {
                Value::Table(deps) => search(deps).transpose(),
                _ => unreachable!("`dev-dependencies` is always a table"),
            })
        )
        .transpose()
}
