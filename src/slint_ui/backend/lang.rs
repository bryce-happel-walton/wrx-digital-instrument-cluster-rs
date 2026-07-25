// TODO: consider switching to Slint's built-in translation
use crate::application::settings::SETTINGS;
use crate::slint_generatedApp::{App, LangResolver};

use rust_embed::Embed;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slint::{ComponentHandle, Weak};
use tokio::select;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Embed)]
#[folder = "resources/lang"]
#[include = "*.lang"]
pub struct Lang;

#[derive(Clone, PartialEq)]
pub struct StrLang(String);

impl std::default::Default for StrLang {
    fn default() -> Self {
        Self(String::from("en_US.lang"))
    }
}

impl std::fmt::Display for StrLang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for StrLang {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // prevent serializing an invalid value
        let selected_resolved = if let Some(_) = Lang::get(self.0.as_str()) {
            self
        } else {
            &Default::default()
        };

        selected_resolved.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StrLang {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|val| Self(val))
    }
}

pub fn process_lang_file(contents: &[u8]) -> HashMap<String, String> {
    let mut ret = HashMap::new();

    match str::from_utf8(contents) {
        Ok(contents) => {
            for line in contents.lines() {
                // comments and blank lines
                if line.is_empty() || line.starts_with("#") {
                    continue;
                } else {
                    if let Some((src, tr)) = line.split_once("=") {
                        ret.insert(src.trim().to_owned(), tr.trim().to_owned());
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to parse .lang file as utf-8: {e:?}")
        }
    }

    ret
}

pub fn bridge(handle_weak: Weak<App>) {
    let translations: Arc<RwLock<HashMap<String, String>>> = Default::default();

    if let Some(handle) = handle_weak.upgrade() {
        let resolver = handle.global::<LangResolver>();

        {
            let translations = translations.clone();
            match slint::spawn_local(async_compat::Compat::new(async move {
                let mut lang_change = SETTINGS.user.general.language.watch();

                loop {
                    let selected_lang = SETTINGS.user.general.language.value();
                    let file = match Lang::get(selected_lang.0.as_str()) {
                        Some(file) => Some(file),
                        None => Lang::get(&StrLang::default().0),
                    };

                    if let Some(file) = file {
                        match translations.write() {
                            Ok(mut translations) => *translations = process_lang_file(&file.data),
                            Err(e) => {
                                eprintln!("Failed to mutate translations: {e:?}")
                            }
                        }
                    }

                    select! {
                        Ok(_) = lang_change.changed() => {},
                        else => {break;}
                    }
                }
            })) {
                Err(e) => eprintln!("Failure in translation manager: {e}"),
                _ => {}
            }
        }

        {
            let translations = translations.clone();
            resolver.on_translate(move |val| match translations.read() {
                Ok(translation) => {
                    if let Some(val) = translation.get(val.as_str()) {
                        val.into()
                    } else {
                        val
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read translations: {e:?}");
                    val
                }
            });
        }
    }
}
