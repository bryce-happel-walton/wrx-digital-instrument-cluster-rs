use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex};
use syn;
use walkdir::WalkDir;

const MANIFEST_DIR: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
/// path to the directory that contains `main.slint`
const SLINT_PATH: LazyLock<PathBuf> = LazyLock::new(|| MANIFEST_DIR.join("src/slint_ui"));
const RESOURCES_PATH: LazyLock<PathBuf> = LazyLock::new(|| MANIFEST_DIR.join("resources"));
const OUT_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| PathBuf::from(std::env::var("OUT_DIR").expect("No output directory")));

/// path to generated files
const GEN_DIR: LazyLock<PathBuf> = LazyLock::new(|| OUT_DIR.join("proj_gen"));
const SLINT_GEN_DIR: LazyLock<PathBuf> = LazyLock::new(|| GEN_DIR.join("slint"));
const CAN_CODEGEN_DIR: LazyLock<PathBuf> = LazyLock::new(|| GEN_DIR.join("can/messages"));

static DBC_DEPS_GEN: LazyLock<Arc<Mutex<Vec<DepEntry>>>> = LazyLock::new(|| Default::default());

struct DepEntry {
    name: String,
    version: String,
    git_rev: Option<String>,
}

static SLINT_LIBRARY_PATHS: LazyLock<HashMap<String, PathBuf>> = LazyLock::new(|| {
    HashMap::from([
        (
            // TODO: make properly organized component library
            "lib".to_string(),
            SLINT_PATH.join("lib/"),
        ),
        ("data".to_string(), SLINT_PATH.join("data/")),
        ("themes".to_string(), SLINT_PATH.join("themes/")),
        ("widgets".to_string(), SLINT_PATH.join("widgets/")),
        // resources
        ("images".to_string(), RESOURCES_PATH.join("images/")),
        ("svg".to_string(), RESOURCES_PATH.join("svg/")),
        ("fonts".to_string(), RESOURCES_PATH.join("fonts/")),
        // generated
        ("gen".to_string(), SLINT_GEN_DIR.clone()),
    ])
});

/// Generates Rust code from dbc files in resources/database/dbc/
///
/// The generated code is placed in src/can/messages/
fn build_dbc() -> Result<(), Box<dyn std::error::Error>> {
    use dbc_codegen::{self, Config};

    let dbc_file_dir = MANIFEST_DIR.join("resources/database/");
    let rs_messages_mod_dir = CAN_CODEGEN_DIR.join("mod.rs");

    let dbc_version_re = regex::Regex::new(r#"pub const VERSION:.*=.*"(.*)";"#).unwrap();

    if CAN_CODEGEN_DIR.exists() {
        fs::remove_dir_all(CAN_CODEGEN_DIR.as_path()).unwrap();
    }
    fs::create_dir_all(CAN_CODEGEN_DIR.as_path()).unwrap();

    let mut mod_file = File::create(rs_messages_mod_dir).unwrap();

    for entry in WalkDir::new(dbc_file_dir) {
        let entry = entry.unwrap();
        let file_name = entry.file_name();
        let entry_path = entry.path();

        if entry_path.extension() == Some(OsStr::new("dbc")) {
            let dbc_name = file_name.to_str().unwrap();
            println!("cargo:rerun-if-changed={}", entry_path.to_str().unwrap());

            let stem = entry_path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .replace(".", "_")
                .replace("-", "_")
                .to_lowercase();

            let out_file_path = CAN_CODEGEN_DIR.join(format!("{stem}.rs"));
            let out_file = File::create(&out_file_path).unwrap();
            let dbc_file = fs::read_to_string(entry_path).unwrap();

            let config = Config::builder()
                .dbc_name(dbc_name)
                .dbc_content(&dbc_file)
                .allow_dead_code(true)
                .check_ranges(dbc_codegen::FeatureConfig::Never)
                .impl_error(dbc_codegen::FeatureConfig::Always)
                .impl_serde(dbc_codegen::FeatureConfig::Always)
                .impl_debug(dbc_codegen::FeatureConfig::Always)
                .build();

            dbc_codegen::codegen(config, &mut BufWriter::new(out_file)).unwrap();

            mod_file
                .write_all(format!("pub mod {stem};\n").as_bytes())
                .unwrap();

            let codegen_file = fs::read_to_string(out_file_path).unwrap();
            let version: String = if let Some(captures) = dbc_version_re.captures(&codegen_file) {
                captures[1].to_string()
            } else {
                "unknown".to_string()
            };

            DBC_DEPS_GEN.lock().unwrap().push(DepEntry {
                name: format!("{stem}_db"),
                version: version.to_string(),
                git_rev: None,
            });
        }
    }

    Ok(())
}

/// Generates Rust code for virtual CAN data generation
fn generate_can_data_emulator() -> Result<(), Box<dyn std::error::Error>> {
    let outputs_dir = GEN_DIR.join("emulators");

    let mod_rs_content = fs::read_to_string(CAN_CODEGEN_DIR.join("mod.rs")).unwrap();
    let mod_rs: syn::File = syn::parse_str(&mod_rs_content).unwrap();

    if outputs_dir.exists() {
        fs::remove_dir_all(&outputs_dir).unwrap();
    }
    fs::create_dir_all(&outputs_dir).unwrap();

    let module_names: Vec<String> = mod_rs
        .items
        .iter()
        .filter_map(|item| {
            if let syn::Item::Mod(syn::ItemMod { ident, .. }) = item {
                Some(ident.to_string())
            } else {
                None
            }
        })
        .collect();

    let mod_in_dir = &outputs_dir.parent().unwrap();
    let mut mod_in_file = File::create(&mod_in_dir.join("mod.rs")).unwrap();
    let mod_out_dir = &outputs_dir.join("mod.rs");
    let mut mod_out_file = File::create(&mod_out_dir).unwrap();

    mod_in_file
        .write(
            format!(
                "pub mod {};\n",
                &outputs_dir.file_name().unwrap().to_str().unwrap()
            )
            .as_bytes(),
        )
        .unwrap();

    for module in module_names {
        let mut gen_output =
            String::from("//! Generated code from build.rs::generate_can_data_emulator()!\n\n"); // full file contents
        let mut gen_block = String::from("let mut ret_frames = vec![];");

        let rs_out_dir = outputs_dir.join(format!("{module}_emulator.rs"));
        let mut rs_out_file = File::create(&rs_out_dir).unwrap();

        let module_content =
            fs::read_to_string(CAN_CODEGEN_DIR.join(format!("{module}.rs"))).unwrap();
        let module_file: syn::File = syn::parse_str(&module_content).unwrap();

        // Find the Messages enum
        let messages_enum = module_file
            .items
            .iter()
            .find_map(|item| {
                if let syn::Item::Enum(syn::ItemEnum {
                    ident, variants, ..
                }) = item
                {
                    if ident == "Messages" {
                        Some(variants)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .expect("Messages enum not found");

        let impls: Vec<&syn::ItemImpl> = module_file
            .items
            .iter()
            .filter_map(|item| {
                if let syn::Item::Impl(item_impl) = item {
                    item_impl
                        .items
                        .iter()
                        .any(|impl_item| {
                            if let syn::ImplItem::Const(constant) = impl_item {
                                constant.ident == "MESSAGE_ID"
                            } else {
                                false
                            }
                        })
                        .then_some(item_impl)
                } else {
                    None
                }
            })
            .collect();

        for (variant, item_impl) in messages_enum.iter().zip(impls.iter()) {
            let signal_path = format!("{module}::{}", variant.ident);

            for impl_item in &item_impl.items {
                if let syn::ImplItem::Fn(func) = impl_item {
                    if func.sig.ident == "new" {
                        let mut param_names: Vec<String> = Vec::new();
                        let mut value_expressions: Vec<String> = Vec::new();

                        for input in &func.sig.inputs {
                            if let syn::FnArg::Typed(pat_type) = input {
                                let param_name = if let syn::Pat::Ident(pat_ident) = &*pat_type.pat
                                {
                                    &pat_ident.ident
                                } else {
                                    continue;
                                };

                                let mut value_expression: String =
                                    format!("let {param_name} = rand::rng()",);

                                if let syn::Type::Path(type_path) = &*pat_type.ty {
                                    if type_path.path.segments.last().unwrap().ident == "bool" {
                                        value_expression += ".random_bool(0.5);";
                                    } else {
                                        let value_ident_path: String = format!(
                                            "{signal_path}::{}",
                                            param_name.to_string().to_uppercase()
                                        );

                                        value_expression += &format!(
                                            ".random_range({0}_MIN..={0}_MAX);",
                                            value_ident_path
                                        )
                                    }
                                }

                                param_names.push(param_name.to_string());
                                value_expressions.push(value_expression);
                            }
                        }

                        let frame_ident =
                            format!("{}_frame", variant.ident.to_string().to_lowercase());
                        let frame_constructor_expression = format!(
                            "\tlet {frame_ident} = {signal_path}::new({}).expect(\"Failed to create frame\");",
                            param_names.join(", ")
                        );
                        let write_frame_expression =
                            format!("\tret_frames.push(CanFrame::from_frame(&{frame_ident}));",);

                        gen_block += &format!(
                            "\t{}\n{frame_constructor_expression}\n{write_frame_expression}\n\n",
                            value_expressions.join("\n\t"),
                        );
                    }
                }
            }
        }
        gen_block += "\tret_frames";

        gen_output += &format!("use crate::can::messages::{module};\n");
        gen_output += "use crate::can::can_backend::CanFrame;\n";
        gen_output += "use rand::RngExt;\n\n";
        gen_output += &format!("pub fn generate_frames() -> Vec<CanFrame> {{\n\t{gen_block}\n}}");

        rs_out_file.write_all(gen_output.as_bytes()).unwrap();
        mod_out_file
            .write(
                format!(
                    "pub mod {};\n",
                    rs_out_dir.file_stem().unwrap().to_str().unwrap()
                )
                .as_bytes(),
            )
            .unwrap();
    }

    Ok(())
}

/// generates slint car data globals
fn generate_slint_car_data() -> Result<(), Box<dyn std::error::Error>> {
    let mod_rs_content = fs::read_to_string(CAN_CODEGEN_DIR.join("mod.rs")).unwrap();
    let mod_rs: syn::File = syn::parse_str(&mod_rs_content).unwrap();

    let module_names: Vec<String> = mod_rs
        .items
        .iter()
        .filter_map(|item| {
            if let syn::Item::Mod(syn::ItemMod { ident, .. }) = item {
                let ident_str = ident.to_string();
                if ident_str != "emulators" {
                    Some(ident_str)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    let mut gen_output = String::new();
    let mut gen_block = String::new();

    for module_name in module_names {
        let module_path = CAN_CODEGEN_DIR.join(format!("{module_name}.rs"));

        let module_content = fs::read_to_string(&module_path).unwrap();
        let module_file: syn::File = syn::parse_str(&module_content).unwrap();

        let messages_enum = module_file
            .items
            .iter()
            .find_map(|item| {
                if let syn::Item::Enum(syn::ItemEnum {
                    ident, variants, ..
                }) = item
                {
                    if ident == "Messages" {
                        Some(variants)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .expect("Messages enum not found");

        let impls: Vec<&syn::ItemImpl> = module_file
            .items
            .iter()
            .filter_map(|item| {
                if let syn::Item::Impl(item_impl) = item {
                    item_impl
                        .items
                        .iter()
                        .any(|impl_item| {
                            if let syn::ImplItem::Const(constant) = impl_item {
                                constant.ident == "MESSAGE_ID"
                            } else {
                                false
                            }
                        })
                        .then_some(item_impl)
                } else {
                    None
                }
            })
            .collect();

        for (variant, item_impl) in messages_enum.iter().zip(impls.iter()) {
            for impl_item in &item_impl.items {
                if let syn::ImplItem::Fn(func) = impl_item {
                    if func.sig.ident == "new" {
                        gen_block += &format!("// {}\n", variant.ident);
                        for input in &func.sig.inputs {
                            if let syn::FnArg::Typed(pat_type) = input {
                                let param_name = if let syn::Pat::Ident(pat_ident) = &*pat_type.pat
                                {
                                    &pat_ident.ident
                                } else {
                                    continue;
                                };

                                // we have to get the actual return type in case the type is an enum
                                // to do this we must search all the functions again for the current
                                // parameter and get the return type of that function
                                for impl_item in &item_impl.items {
                                    if let syn::ImplItem::Fn(func) = impl_item {
                                        if &func.sig.ident == param_name {
                                            match &func.sig.output {
                                                syn::ReturnType::Type(_, ty) => {
                                                    if let syn::Type::Path(type_path) = &**ty {
                                                        let (slint_type, init_value) =
                                                            match type_path
                                                                .path
                                                                .segments
                                                                .last()
                                                                .unwrap()
                                                                .ident
                                                                .to_string()
                                                                .as_str()
                                                            {
                                                                "bool" => {
                                                                    ("SBDataParameter", "true")
                                                                }
                                                                "u8" | "u16" | "u32" | "u64"
                                                                | "i8" | "i16" | "i32" | "i64" => {
                                                                    ("SIDataParameter", "100")
                                                                }
                                                                "f32" | "f64" => {
                                                                    ("SFDataParameter", "100.4")
                                                                }
                                                                //-// ! must implement into<SharedString> for all enum types
                                                                _ => (
                                                                    "SStrDataParameter",
                                                                    "\"?VAL?\"",
                                                                ),
                                                            };

                                                        gen_block += &format!(
                                                            "\tin property <{slint_type}> {param_name}: {{ value: {init_value}"
                                                        );
                                                        match slint_type {
                                                            "SIDataParameter"
                                                            | "SFDataParameter" => {
                                                                gen_block += ", max: 9999, unit_str: \"?UNIT?\"";
                                                            }
                                                            _ => {}
                                                        }
                                                        gen_block += " };\n";
                                                    };
                                                }
                                                _ => {}
                                            };
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    gen_output += "/// autogenerated code from build.rs::generate_slint_car_data()!\n";
    gen_output += "import {SFDataParameter, SIDataParameter, SBDataParameter, SStrDataParameter} from \"@data/data_parameter.slint\";\n";
    gen_output += "export global SCarData {\n";
    gen_output += &gen_block;
    gen_output += "}";

    let slint_out_dir = SLINT_GEN_DIR.join("data/car_data.slint");
    if let Some(parent) = slint_out_dir.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut slint_out_file = File::create(slint_out_dir).unwrap();

    slint_out_file.write(gen_output.as_bytes()).unwrap();

    Ok(())
}

fn capitalize_first_words(s: &str) -> String {
    let mut words_capitilized: Vec<String> = vec![];

    for word in s.split('_').collect::<Vec<_>>() {
        let mut chars = word.chars();
        match chars.next() {
            None => {}
            Some(c) => {
                words_capitilized.push(c.to_uppercase().collect::<String>() + chars.as_str());
            }
        }
    }

    words_capitilized.join("")
}

fn generate_slint_themes() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: generate libpaths so different themes can include each other without dependency loops
    let themes_dir = SLINT_PATH.join("themes");
    themes_dir
        .try_exists()
        .expect("Slint themes dir does not exist");

    println!(
        "cargo:rerun-if-changed={}",
        themes_dir.join("themes.slint").to_str().unwrap()
    );

    let mut gen_output = String::from("/// autogenerated from build.rs::generate_slint_themes()\n");
    gen_output += "\n//! See note in themes.slint\n\n";

    let mut theme_components: Vec<String> = vec![];
    let mut theme_entries: Vec<std::fs::DirEntry> = fs::read_dir(&themes_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .collect();

    theme_entries.sort_by(|a, b| {
        a.file_name()
            .to_ascii_lowercase()
            .cmp(&b.file_name().to_ascii_lowercase())
    });

    for entry in theme_entries {
        let path = entry.path();

        if let Some(parent_dir) = path.file_name() {
            let parent_dir = parent_dir.to_string_lossy().into_owned();
            if path.is_dir() {
                for entry in path.read_dir().unwrap().filter_map(Result::ok) {
                    let path = entry.path();

                    if path.is_file()
                        && path.extension().is_some_and(|ext| ext == "slint")
                        && path
                            .file_stem()
                            .is_some_and(|stem| *stem == *format!("{parent_dir}_main"))
                    {
                        let theme_component = capitalize_first_words(&parent_dir) + "Theme";
                        gen_output += &format!(
                            "import {{ {theme_component} }} from \"@themes/{parent_dir}/{parent_dir}_main.slint\";\n"
                        );
                        theme_components.push(theme_component);
                    }
                }
            }
        }
    }

    gen_output += &format!(
        "\n@rust-attr(derive(strum::VariantArray, serde::Serialize, serde::Deserialize))\nexport enum ClusterTheme {{\n{}\n}}\n",
        theme_components
            .iter()
            .map(|val| format!("\t{},", val.strip_suffix("Theme").unwrap()))
            .collect::<Vec<_>>()
            .join("\n")
    );
    gen_output += "\nexport global GlobalThemeData {\n";
    gen_output += "\tin-out property <ClusterTheme> current-theme: ClusterTheme.Default;\n";
    gen_output += "\tcallback update_current_theme(ClusterTheme);\n";
    gen_output += "}\n";

    gen_output += "\nexport component ThemeLoader {\n\twidth: 100%;\n\theight: 100%;\n\n";

    for theme_component in theme_components {
        let theme_enum = theme_component.strip_suffix("Theme").unwrap();
        gen_output += &format!(
            "\tif GlobalThemeData.current-theme == ClusterTheme.{theme_enum}: {theme_component} {{width: 100%; height: 100%;}}\n"
        );
    }

    gen_output += "}";

    let slint_out_dir = SLINT_GEN_DIR.join("themes/theme_loader_gen.slint");
    if let Some(parent) = slint_out_dir.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut slint_out_file = File::create(slint_out_dir).unwrap();

    slint_out_file.write(gen_output.as_bytes()).unwrap();

    Ok(())
}

fn update_vscode_slint_libpaths() {
    const SETTING_NAME: &str = "slint.libraryPaths";

    let vscode_settings = MANIFEST_DIR.join(".vscode/settings.json");

    if let Some(parent) = vscode_settings.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    if !vscode_settings.exists() {
        File::create(&vscode_settings).unwrap();
    }

    println!(
        "cargo:rerun-if-changed={}",
        vscode_settings.to_str().unwrap()
    );

    let contents = fs::read_to_string(&vscode_settings).unwrap_or_default();
    let mut settings = if contents.trim().is_empty() {
        serde_json::json!({})
    } else {
        match json5::from_str::<serde_json::Value>(&contents) {
            Ok(v) => v,
            Err(_) => {
                println!(
                    "cargo:warning=Failed to parse json {}",
                    vscode_settings.to_string_lossy()
                );
                return;
            }
        }
    };

    let lib_paths = settings
        .as_object_mut()
        .unwrap()
        .entry(SETTING_NAME)
        .or_insert_with(|| serde_json::json!({}));

    let map = lib_paths.as_object_mut().unwrap();
    map.retain(|key, _| SLINT_LIBRARY_PATHS.contains_key(key));

    for (key, path) in SLINT_LIBRARY_PATHS.iter() {
        // Point the IDE at the source SVGs, not the build-time flattened copies
        // under OUT_DIR (which may not exist on a fresh checkout).
        let resolved = if key == "svg" {
            RESOURCES_PATH.join("svg/")
        } else {
            path.clone()
        };
        let value = resolved
            .strip_prefix(MANIFEST_DIR.as_path())
            .unwrap_or(&resolved)
            .to_string_lossy();
        map.insert(key.clone(), value.into_owned().into());
    }

    let output = serde_json::to_string_pretty(&settings).unwrap();
    fs::write(vscode_settings, output).unwrap();
}

fn generate_dependencies_rs() {
    let metadata = cargo_metadata::MetadataCommand::new().exec().unwrap();

    let root_pkg = metadata.root_package().expect("No root package found");

    let root_id = &root_pkg.id;
    let crate_name = &root_pkg.name;
    let crate_version = root_pkg.version.to_string();

    let crate_git_rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .map(|r| r[..7.min(r.len())].to_string());

    let dbc_git_rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(RESOURCES_PATH.join("database/CAN_database"))
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .map(|r| r[..7.min(r.len())].to_string());

    let root_node = metadata
        .resolve
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|n| &n.id == root_id)
        .expect("Root node not found in resolve graph");

    let direct_dep_ids: std::collections::HashSet<&cargo_metadata::PackageId> =
        root_node.deps.iter().map(|d| &d.pkg).collect();

    let mut deps: Vec<DepEntry> = metadata
        .packages
        .iter()
        .filter(|p| direct_dep_ids.contains(&p.id))
        .map(|p| {
            let git_rev = p.source.as_ref().and_then(|s| {
                if s.repr.starts_with("git+") {
                    s.repr
                        .rsplit('#')
                        .next()
                        .map(|r| r[..7.min(r.len())].to_string())
                } else {
                    None
                }
            });
            DepEntry {
                name: p.name.to_string(),
                version: p.version.to_string(),
                git_rev,
            }
        })
        .collect();

    deps.sort_by(|a, b| a.name.cmp(&b.name));

    let mut crate_deps = vec![
        DepEntry {
            name: crate_name.to_string(),
            version: crate_version,
            git_rev: crate_git_rev,
        },
        DepEntry {
            name: "CAN_database".to_string(),
            version: "".to_string(),
            git_rev: dbc_git_rev,
        },
    ];

    crate_deps.extend(std::mem::take(&mut *DBC_DEPS_GEN.lock().unwrap()));

    // Insert the current crate and our deps at the front
    deps.splice(0..0, crate_deps);

    let mut out = String::from(
        "/// autogenerated code from build.rs:: build.rs::generate_dependencies_rs()\n\n",
    );
    out += "#[allow(unused)]\n";
    out += "#[derive(Debug)]\n";
    out += "pub struct DependencyInfo {\n";
    out += "    pub name: &'static str,\n";
    out += "    pub version: &'static str,\n";
    out += "    pub git_rev: Option<&'static str>,\n";
    out += "}\n\n";

    // Typed struct with a pub field per dependency
    out += "#[allow(unused, non_snake_case)]\n";
    out += "pub struct Dependencies {\n";
    for dep in &deps {
        let field_name = dep.name.replace('-', "_");
        out += &format!("    pub {field_name}: DependencyInfo,\n");
    }
    out += "}\n\n";

    let dep_count = deps.len();
    out += "#[allow(unused)]\n";
    out += "impl Dependencies {\n";
    out += "    /// Returns an iterable slice containing the dependencies and the crate\n";
    out += &format!("    pub const fn as_slice(&self) -> [&DependencyInfo; {dep_count}] {{[\n");
    for dep in &deps {
        let field_name = dep.name.replace('-', "_");
        out += &format!("       &self.{field_name},\n");
    }
    out += "    ]}\n";
    out += "}\n\n";

    out += "#[allow(unused, non_snake_case)]\n";
    out += "pub const DEPENDENCIES: Dependencies = Dependencies {\n";
    for dep in &deps {
        let field_name = dep.name.replace('-', "_");
        let rev_str = match &dep.git_rev {
            Some(rev) => format!("Some(\"{rev}\")"),
            None => "None".to_string(),
        };
        out += &format!(
            "    {field_name}: DependencyInfo {{ name: \"{}\", version: \"{}\", git_rev: {rev_str} }},\n",
            dep.name, dep.version,
        );
    }
    out += "};\n";

    fs::write(OUT_DIR.join("dependencies.rs"), out).expect("Failed to write dependencies.rs");
}

fn main() {
    build_dbc().unwrap();
    generate_can_data_emulator().unwrap();
    generate_slint_car_data().unwrap();
    generate_slint_themes().unwrap();
    update_vscode_slint_libpaths();
    generate_dependencies_rs();

    let settings = slint_build::CompilerConfiguration::new()
        .with_library_paths(LazyLock::force(&SLINT_LIBRARY_PATHS).clone());
    slint_build::compile_with_config(SLINT_PATH.join("main.slint"), settings).unwrap();
}
