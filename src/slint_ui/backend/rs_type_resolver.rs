use crate::data::parameters::Bound;
use crate::data::units::UnitSystem;
use crate::slint_generatedApp::{App, ClusterTheme, RSTypeResolver, SBoundInt};
use crate::slint_ui::backend::lang::{Lang, StrLang};

use slint::{ComponentHandle, SharedString, Weak};
use strum::VariantArray;

macro_rules! variants_match_as_model {
    ($val:ident => {$($ty:tt),*}) => {
        match $val.as_str() {
            $(
                stringify!($ty) => $ty::VARIANTS
                    .iter()
                    .map(|var| format!("{var}").into())
                    .collect::<Vec<SharedString>>()
                    .as_slice()
                    .into(),
            )*
            _ => ["error_variant".into()].into()
        }
    };
}

pub fn bridge(handle_weak: Weak<App>) {
    if let Some(handle) = handle_weak.upgrade() {
        let resolver = handle.global::<RSTypeResolver>();

        resolver.on_serialize_sboundint(|value| {
            let value = Bound::<i32>::from(value);
            serde_json::to_string(&value).unwrap_or_default().into()
        });

        resolver
            .on_serialize_bool(|value| serde_json::to_string(&value).unwrap_or_default().into());

        resolver.on_jsonify_str(|value| format!("\"{value}\"").into()); // add quotation marks for json serialization

        resolver.on_dejsonify_str(|value| {
            if value.starts_with("\"") && value.ends_with("\"") {
                &value[1..(value.len() - 1)] // remove the quotation marks from json values
            } else {
                &value
            }
            .into()
        });

        resolver.on_get_enum_variants(|ty| match ty.as_str().trim() {
            stringify!(StrLang) => Lang::iter()
                .map(|var| SharedString::from(var.as_ref()))
                .collect::<Vec<SharedString>>()
                .as_slice()
                .into(),
            _ => variants_match_as_model!(ty => {
                UnitSystem,
                ClusterTheme
            }),
        });
    }
}

macro_rules! generate_bound_type {
    ($($ty:ty $(=> $bound_ty:path)?),* $(,)*) => {
        $(impl From<Bound<$ty>> for $ty {
            fn from(b: Bound<$ty>) -> Self {
                b.value()
            }
        }

        $(
            impl Into<$bound_ty> for Bound<$ty> {
                fn into(self) -> $bound_ty {
                    $bound_ty {
                        end: self.end(),
                        step: self.step(),
                        start: self.start(),
                        value: self.value()
                    }
                }
            }

            impl From<$bound_ty> for Bound<$ty> {
                fn from(value: $bound_ty) -> Self {
                    Self::new(value.value, value.start..=value.end, value.step)
                }
            }
        )?)*
    };
}

generate_bound_type!(
    i32 => SBoundInt,
    f32,
);
